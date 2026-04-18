pub const VRAM_WIDTH: usize = 1024;
pub const VRAM_HEIGHT: usize = 512;

const GPU_STATUS_READY: u32 = 0x1f00_0000;

#[derive(Clone, Copy, Debug, Default)]
struct TexturePage {
    x_base: u16, // VRAM pixel x (multiple of 64)
    y_base: u16, // VRAM pixel y (0 or 256)
    depth: u8,   // 0=4bit CLUT, 1=8bit CLUT, 2=15bit direct
}

impl TexturePage {
    fn update_from_attr(&mut self, attr: u16) {
        self.x_base = (attr & 0xf) * 64;
        self.y_base = ((attr >> 4) & 1) * 256;
        self.depth = ((attr >> 7) & 3) as u8;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gp0Command {
    Nop,
    ClearCache,
    FillRectangle,
    // Flat-shaded polygons
    MonoTriangle,
    MonoQuad,
    // Textured flat-shaded polygons (opcode byte carried for raw/semi-trans decode)
    TexturedTriangle,
    TexturedQuad,
    // Gouraud-shaded polygons
    GouraudTriangle,
    GouraudQuad,
    // Gouraud + textured
    GouraudTexturedTriangle,
    GouraudTexturedQuad,
    // Rectangles: opcode byte encodes size (bits 4:3) and texture/semi-trans (bits 2:0)
    Rect(u8),
    // VRAM transfers
    VramToVram,
    CpuToVram,
    VramToCpu,
    // GPU state
    DrawMode,
    TextureWindow,
    DrawingAreaTopLeft,
    DrawingAreaBottomRight,
    DrawingOffset,
    MaskBitSetting,
    Unknown(u8),
}

impl Gp0Command {
    fn from_word(word: u32) -> Self {
        match (word >> 24) as u8 {
            0x00 => Self::Nop,
            0x01 => Self::ClearCache,
            0x02 => Self::FillRectangle,
            0x20..=0x23 => Self::MonoTriangle,
            0x24..=0x27 => Self::TexturedTriangle,
            0x28..=0x2b => Self::MonoQuad,
            0x2c..=0x2f => Self::TexturedQuad,
            0x30..=0x33 => Self::GouraudTriangle,
            0x34..=0x37 => Self::GouraudTexturedTriangle,
            0x38..=0x3b => Self::GouraudQuad,
            0x3c..=0x3f => Self::GouraudTexturedQuad,
            op @ 0x60..=0x7f => Self::Rect(op),
            0x80 => Self::VramToVram,
            0xa0 => Self::CpuToVram,
            0xc0 => Self::VramToCpu,
            0xe1 => Self::DrawMode,
            0xe2 => Self::TextureWindow,
            0xe3 => Self::DrawingAreaTopLeft,
            0xe4 => Self::DrawingAreaBottomRight,
            0xe5 => Self::DrawingOffset,
            0xe6 => Self::MaskBitSetting,
            opcode => Self::Unknown(opcode),
        }
    }

    fn word_count(self) -> usize {
        match self {
            Self::Nop
            | Self::ClearCache
            | Self::DrawMode
            | Self::TextureWindow
            | Self::DrawingAreaTopLeft
            | Self::DrawingAreaBottomRight
            | Self::DrawingOffset
            | Self::MaskBitSetting
            | Self::Unknown(_) => 1,
            // color + 3 vertices
            Self::MonoTriangle => 4,
            // color + 3*(xy + uv_clut/tpage)
            Self::TexturedTriangle => 7,
            // color + 4 vertices
            Self::MonoQuad => 5,
            // color + 4*(xy + uv)
            Self::TexturedQuad => 9,
            // 3*(color + xy)
            Self::GouraudTriangle => 6,
            // 4*(color + xy)
            Self::GouraudQuad => 8,
            // 3*(color + xy + uv)
            Self::GouraudTexturedTriangle => 9,
            // 4*(color + xy + uv)
            Self::GouraudTexturedQuad => 12,
            // base=2 (color+xy), +1 if textured (uv+clut word), +1 if variable size (wh word)
            Self::Rect(op) => {
                let textured = (op >> 2) & 1 != 0;
                let variable = (op >> 3) & 3 == 0;
                2 + textured as usize + variable as usize
            }
            Self::FillRectangle | Self::CpuToVram | Self::VramToCpu => 3,
            Self::VramToVram => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ImageLoad {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    written_pixels: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DrawingArea {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

pub struct Gpu {
    vram: Box<[u16; VRAM_WIDTH * VRAM_HEIGHT]>,
    gp0_words: Vec<u32>,
    image_load: Option<ImageLoad>,
    drawing_area: DrawingArea,
    drawing_offset_x: i32,
    drawing_offset_y: i32,
    tpage: TexturePage,
    display_start_x: u16,
    display_start_y: u16,
    display_width: u16,
    display_height: u16,
    status: u32,
    command_count: u64,
    draw_count: u64,
    image_upload_count: u64,
    unknown_command_count: u64,
    last_command: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuDebugState {
    pub command_count: u64,
    pub draw_count: u64,
    pub image_upload_count: u64,
    pub unknown_command_count: u64,
    pub last_command: Option<u8>,
}

impl Gpu {
    pub fn new() -> Self {
        Self {
            vram: vec![0; VRAM_WIDTH * VRAM_HEIGHT]
                .into_boxed_slice()
                .try_into()
                .expect("VRAM allocation must match dimensions"),
            gp0_words: Vec::new(),
            image_load: None,
            drawing_area: DrawingArea {
                left: 0,
                top: 0,
                right: VRAM_WIDTH as i32 - 1,
                bottom: VRAM_HEIGHT as i32 - 1,
            },
            drawing_offset_x: 0,
            drawing_offset_y: 0,
            tpage: TexturePage::default(),
            display_start_x: 0,
            display_start_y: 0,
            display_width: 320,
            display_height: 240,
            status: GPU_STATUS_READY,
            command_count: 0,
            draw_count: 0,
            image_upload_count: 0,
            unknown_command_count: 0,
            last_command: None,
        }
    }

    pub fn read_status(&self) -> u32 {
        self.status
    }

    pub fn write_gp0(&mut self, word: u32) {
        if self.image_load.is_some() {
            self.write_image_word(word);
            return;
        }

        self.gp0_words.push(word);
        let command = Gp0Command::from_word(self.gp0_words[0]);
        if self.gp0_words.len() < command.word_count() {
            return;
        }

        let words = self.gp0_words.clone();
        self.gp0_words.clear();
        self.execute_gp0(command, &words);
    }

    pub fn write_gp1(&mut self, word: u32) {
        match (word >> 24) as u8 {
            0x00 => self.soft_reset(),
            0x01 => self.gp0_words.clear(),
            0x02 => self.status |= 1 << 26,
            0x03 => {} // display enable
            0x04 => {} // DMA direction
            0x05 => {
                self.display_start_x = (word & 0x3ff) as u16;
                self.display_start_y = ((word >> 10) & 0x1ff) as u16;
            }
            0x06 => {} // horizontal display range
            0x07 => {} // vertical display range
            0x08 => {
                let hres_table = [256u16, 320, 512, 640];
                let hres = if (word >> 6) & 1 != 0 {
                    368
                } else {
                    hres_table[(word & 3) as usize]
                };
                let vres = if (word >> 2) & 1 != 0 { 480u16 } else { 240 };
                self.display_width = hres;
                self.display_height = vres;
            }
            _ => {}
        }
    }

    pub fn display_width(&self) -> usize {
        self.display_width as usize
    }

    pub fn display_height(&self) -> usize {
        self.display_height as usize
    }

    /// Returns the active display region as packed RGB24 bytes (width * height * 3).
    pub fn framebuffer_rgb(&self) -> Vec<u8> {
        let w = self.display_width as usize;
        let h = self.display_height as usize;
        let sx = self.display_start_x as usize;
        let sy = self.display_start_y as usize;
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let vx = (sx + x) % VRAM_WIDTH;
                let vy = (sy + y) % VRAM_HEIGHT;
                let [r, g, b] = rgb555_to_rgb888(self.vram[vy * VRAM_WIDTH + vx]);
                rgb.extend_from_slice(&[r, g, b]);
            }
        }
        rgb
    }

    pub fn debug_state(&self) -> GpuDebugState {
        GpuDebugState {
            command_count: self.command_count,
            draw_count: self.draw_count,
            image_upload_count: self.image_upload_count,
            unknown_command_count: self.unknown_command_count,
            last_command: self.last_command,
        }
    }

    /// GP1(00) soft reset: resets controller state but does NOT clear VRAM.
    fn soft_reset(&mut self) {
        self.gp0_words.clear();
        self.image_load = None;
        self.drawing_area = DrawingArea {
            left: 0,
            top: 0,
            right: VRAM_WIDTH as i32 - 1,
            bottom: VRAM_HEIGHT as i32 - 1,
        };
        self.drawing_offset_x = 0;
        self.drawing_offset_y = 0;
        self.tpage = TexturePage::default();
        self.status = GPU_STATUS_READY;
    }

    fn execute_gp0(&mut self, command: Gp0Command, words: &[u32]) {
        self.command_count += 1;
        self.last_command = Some((words[0] >> 24) as u8);
        match command {
            Gp0Command::Nop | Gp0Command::ClearCache => {}

            Gp0Command::FillRectangle => {
                self.draw_count += 1;
                let color = color24_to_rgb555(words[0]);
                let (x, y) = xy(words[1]);
                let (width, height) = xy(words[2]);
                self.fill_rect_raw(x, y, width.max(1), height.max(1), color);
            }

            Gp0Command::MonoTriangle => {
                self.draw_count += 1;
                let color = color24_to_rgb555(words[0]);
                self.draw_triangle(color, [vertex(words[1]), vertex(words[2]), vertex(words[3])]);
            }

            Gp0Command::MonoQuad => {
                self.draw_count += 1;
                let color = color24_to_rgb555(words[0]);
                let [a, b, c, d] = [
                    vertex(words[1]),
                    vertex(words[2]),
                    vertex(words[3]),
                    vertex(words[4]),
                ];
                self.draw_triangle(color, [a, b, c]);
                self.draw_triangle(color, [b, c, d]);
            }

            Gp0Command::TexturedTriangle => {
                self.draw_count += 1;
                let mod_color = words[0] & 0x00ff_ffff;
                let v0 = vertex(words[1]);
                let (u0, v0t) = parse_uv(words[2]);
                let clut_x = ((words[2] >> 16) as u16 & 0x3f) * 16;
                let clut_y = ((words[2] >> 22) & 0x1ff) as u16;
                let v1 = vertex(words[3]);
                let (u1, v1t) = parse_uv(words[4]);
                self.tpage.update_from_attr((words[4] >> 16) as u16);
                let v2 = vertex(words[5]);
                let (u2, v2t) = parse_uv(words[6]);
                self.draw_textured_triangle(
                    mod_color,
                    [v0, v1, v2],
                    [(u0, v0t), (u1, v1t), (u2, v2t)],
                    clut_x,
                    clut_y,
                );
            }

            Gp0Command::TexturedQuad => {
                self.draw_count += 1;
                let mod_color = words[0] & 0x00ff_ffff;
                let v0 = vertex(words[1]);
                let (u0, v0t) = parse_uv(words[2]);
                let clut_x = ((words[2] >> 16) as u16 & 0x3f) * 16;
                let clut_y = ((words[2] >> 22) & 0x1ff) as u16;
                let v1 = vertex(words[3]);
                let (u1, v1t) = parse_uv(words[4]);
                self.tpage.update_from_attr((words[4] >> 16) as u16);
                let v2 = vertex(words[5]);
                let (u2, v2t) = parse_uv(words[6]);
                let v3 = vertex(words[7]);
                let (u3, v3t) = parse_uv(words[8]);
                self.draw_textured_triangle(
                    mod_color,
                    [v0, v1, v2],
                    [(u0, v0t), (u1, v1t), (u2, v2t)],
                    clut_x,
                    clut_y,
                );
                self.draw_textured_triangle(
                    mod_color,
                    [v1, v2, v3],
                    [(u1, v1t), (u2, v2t), (u3, v3t)],
                    clut_x,
                    clut_y,
                );
            }

            Gp0Command::GouraudTriangle => {
                self.draw_count += 1;
                let colors = [
                    words[0] & 0x00ff_ffff,
                    words[2] & 0x00ff_ffff,
                    words[4] & 0x00ff_ffff,
                ];
                let pts = [vertex(words[1]), vertex(words[3]), vertex(words[5])];
                self.draw_gouraud_triangle(colors, pts);
            }

            Gp0Command::GouraudQuad => {
                self.draw_count += 1;
                let colors = [
                    words[0] & 0x00ff_ffff,
                    words[2] & 0x00ff_ffff,
                    words[4] & 0x00ff_ffff,
                    words[6] & 0x00ff_ffff,
                ];
                let pts = [
                    vertex(words[1]),
                    vertex(words[3]),
                    vertex(words[5]),
                    vertex(words[7]),
                ];
                self.draw_gouraud_triangle(
                    [colors[0], colors[1], colors[2]],
                    [pts[0], pts[1], pts[2]],
                );
                self.draw_gouraud_triangle(
                    [colors[1], colors[2], colors[3]],
                    [pts[1], pts[2], pts[3]],
                );
            }

            Gp0Command::GouraudTexturedTriangle => {
                self.draw_count += 1;
                let colors = [
                    words[0] & 0x00ff_ffff,
                    words[3] & 0x00ff_ffff,
                    words[6] & 0x00ff_ffff,
                ];
                let v0 = vertex(words[1]);
                let (u0, v0t) = parse_uv(words[2]);
                let clut_x = ((words[2] >> 16) as u16 & 0x3f) * 16;
                let clut_y = ((words[2] >> 22) & 0x1ff) as u16;
                let v1 = vertex(words[4]);
                let (u1, v1t) = parse_uv(words[5]);
                self.tpage.update_from_attr((words[5] >> 16) as u16);
                let v2 = vertex(words[7]);
                let (u2, v2t) = parse_uv(words[8]);
                self.draw_gouraud_textured_triangle(
                    colors,
                    [v0, v1, v2],
                    [(u0, v0t), (u1, v1t), (u2, v2t)],
                    clut_x,
                    clut_y,
                );
            }

            Gp0Command::GouraudTexturedQuad => {
                self.draw_count += 1;
                let colors = [
                    words[0] & 0x00ff_ffff,
                    words[3] & 0x00ff_ffff,
                    words[6] & 0x00ff_ffff,
                    words[9] & 0x00ff_ffff,
                ];
                let v0 = vertex(words[1]);
                let (u0, v0t) = parse_uv(words[2]);
                let clut_x = ((words[2] >> 16) as u16 & 0x3f) * 16;
                let clut_y = ((words[2] >> 22) & 0x1ff) as u16;
                let v1 = vertex(words[4]);
                let (u1, v1t) = parse_uv(words[5]);
                self.tpage.update_from_attr((words[5] >> 16) as u16);
                let v2 = vertex(words[7]);
                let (u2, v2t) = parse_uv(words[8]);
                let v3 = vertex(words[10]);
                let (u3, v3t) = parse_uv(words[11]);
                self.draw_gouraud_textured_triangle(
                    [colors[0], colors[1], colors[2]],
                    [v0, v1, v2],
                    [(u0, v0t), (u1, v1t), (u2, v2t)],
                    clut_x,
                    clut_y,
                );
                self.draw_gouraud_textured_triangle(
                    [colors[1], colors[2], colors[3]],
                    [v1, v2, v3],
                    [(u1, v1t), (u2, v2t), (u3, v3t)],
                    clut_x,
                    clut_y,
                );
            }

            Gp0Command::Rect(op) => {
                self.draw_count += 1;
                let is_textured = (op >> 2) & 1 != 0;
                let size_bits = (op >> 3) & 3;

                let mod_color = words[0] & 0x00ff_ffff;
                let flat_color = color24_to_rgb555(words[0]);
                let (x, y) = vertex(words[1]);

                let (u0, v0, clut_x, clut_y) = if is_textured {
                    let uv_word = words[2];
                    let u = (uv_word & 0xff) as u8;
                    let v = ((uv_word >> 8) & 0xff) as u8;
                    let clut_x = ((uv_word >> 16) as u16 & 0x3f) * 16;
                    let clut_y = ((uv_word >> 22) & 0x1ff) as u16;
                    (u, v, clut_x, clut_y)
                } else {
                    (0, 0, 0, 0)
                };

                // Width/height: fixed sizes or variable (from the next word after UV)
                let size_word_idx = if is_textured { 3 } else { 2 };
                let (width, height): (u32, u32) = match size_bits {
                    0 => {
                        let (w, h) = xy(words[size_word_idx]);
                        (w.max(1), h.max(1))
                    }
                    1 => (1, 1),
                    2 => (8, 8),
                    3 => (16, 16),
                    _ => unreachable!(),
                };

                if is_textured {
                    self.draw_textured_rect(mod_color, x, y, u0, v0, width, height, clut_x, clut_y);
                } else {
                    self.fill_rect(x, y, width, height, flat_color);
                }
            }

            Gp0Command::VramToVram => {
                let (src_x, src_y) = xy(words[1]);
                let (dst_x, dst_y) = xy(words[2]);
                let (width, height) = xy(words[3]);
                self.copy_vram(
                    src_x as usize,
                    src_y as usize,
                    dst_x as usize,
                    dst_y as usize,
                    width.max(1) as usize,
                    height.max(1) as usize,
                );
            }

            Gp0Command::CpuToVram => {
                self.image_upload_count += 1;
                let (x, y) = xy(words[1]);
                let (width, height) = xy(words[2]);
                self.image_load = Some(ImageLoad {
                    x: x as usize,
                    y: y as usize,
                    width: width.max(1) as usize,
                    height: height.max(1) as usize,
                    written_pixels: 0,
                });
            }

            Gp0Command::VramToCpu => {
                // GPU-to-CPU readback: not yet implemented (no GP0 read path in bus)
            }

            Gp0Command::DrawingAreaTopLeft => {
                self.drawing_area.left = (words[0] & 0x3ff) as i32;
                self.drawing_area.top = ((words[0] >> 10) & 0x1ff) as i32;
            }
            Gp0Command::DrawingAreaBottomRight => {
                self.drawing_area.right = (words[0] & 0x3ff) as i32;
                self.drawing_area.bottom = ((words[0] >> 10) & 0x1ff) as i32;
            }
            Gp0Command::DrawingOffset => {
                self.drawing_offset_x = sign_extend(words[0] & 0x7ff, 11);
                self.drawing_offset_y = sign_extend((words[0] >> 11) & 0x7ff, 11);
            }
            Gp0Command::DrawMode => {
                self.tpage.update_from_attr(words[0] as u16);
            }
            Gp0Command::TextureWindow | Gp0Command::MaskBitSetting => {}
            Gp0Command::Unknown(_) => self.unknown_command_count += 1,
        }
    }

    fn write_image_word(&mut self, word: u32) {
        let mut load = self.image_load.expect("image load state exists");
        let total_pixels = load.width * load.height;
        for pixel in [word as u16, (word >> 16) as u16] {
            if load.written_pixels >= total_pixels {
                break;
            }
            let x = load.x + load.written_pixels % load.width;
            let y = load.y + load.written_pixels / load.width;
            self.write_vram(x, y, pixel);
            load.written_pixels += 1;
        }
        self.image_load = (load.written_pixels < total_pixels).then_some(load);
    }

    // ---- Rasterizers ----

    fn draw_triangle(&mut self, color: u16, points: [(i32, i32); 3]) {
        let pts = points.map(|(x, y)| (x + self.drawing_offset_x, y + self.drawing_offset_y));
        let min_x = pts.iter().map(|(x, _)| *x).min().unwrap().max(self.drawing_area.left);
        let max_x = pts.iter().map(|(x, _)| *x).max().unwrap().min(self.drawing_area.right);
        let min_y = pts.iter().map(|(_, y)| *y).min().unwrap().max(self.drawing_area.top);
        let max_y = pts.iter().map(|(_, y)| *y).max().unwrap().min(self.drawing_area.bottom);

        let area = edge(pts[0], pts[1], pts[2]);
        if area == 0 {
            return;
        }

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let p = (x, y);
                let w0 = edge(pts[1], pts[2], p);
                let w1 = edge(pts[2], pts[0], p);
                let w2 = edge(pts[0], pts[1], p);
                if (w0 >= 0 && w1 >= 0 && w2 >= 0) || (w0 <= 0 && w1 <= 0 && w2 <= 0) {
                    self.write_draw_pixel(x, y, color);
                }
            }
        }
    }

    fn draw_gouraud_triangle(&mut self, colors: [u32; 3], points: [(i32, i32); 3]) {
        let pts = points.map(|(x, y)| (x + self.drawing_offset_x, y + self.drawing_offset_y));
        let min_x = pts.iter().map(|(x, _)| *x).min().unwrap().max(self.drawing_area.left);
        let max_x = pts.iter().map(|(x, _)| *x).max().unwrap().min(self.drawing_area.right);
        let min_y = pts.iter().map(|(_, y)| *y).min().unwrap().max(self.drawing_area.top);
        let max_y = pts.iter().map(|(_, y)| *y).max().unwrap().min(self.drawing_area.bottom);

        let area = edge(pts[0], pts[1], pts[2]);
        if area == 0 {
            return;
        }
        let area64 = area as i64;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let p = (x, y);
                let w0 = edge(pts[1], pts[2], p) as i64;
                let w1 = edge(pts[2], pts[0], p) as i64;
                let w2 = edge(pts[0], pts[1], p) as i64;
                if (w0 >= 0 && w1 >= 0 && w2 >= 0) || (w0 <= 0 && w1 <= 0 && w2 <= 0) {
                    let r = interp8(colors, w0, w1, w2, area64, 0);
                    let g = interp8(colors, w0, w1, w2, area64, 8);
                    let b = interp8(colors, w0, w1, w2, area64, 16);
                    let color = color24_to_rgb555(r | (g << 8) | (b << 16));
                    self.write_draw_pixel(x, y, color);
                }
            }
        }
    }

    fn draw_textured_triangle(
        &mut self,
        mod_color: u32,
        points: [(i32, i32); 3],
        uvs: [(u8, u8); 3],
        clut_x: u16,
        clut_y: u16,
    ) {
        let pts = points.map(|(x, y)| (x + self.drawing_offset_x, y + self.drawing_offset_y));
        let min_x = pts.iter().map(|(x, _)| *x).min().unwrap().max(self.drawing_area.left);
        let max_x = pts.iter().map(|(x, _)| *x).max().unwrap().min(self.drawing_area.right);
        let min_y = pts.iter().map(|(_, y)| *y).min().unwrap().max(self.drawing_area.top);
        let max_y = pts.iter().map(|(_, y)| *y).max().unwrap().min(self.drawing_area.bottom);

        let area = edge(pts[0], pts[1], pts[2]);
        if area == 0 {
            return;
        }
        let area64 = area as i64;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let p = (x, y);
                let w0 = edge(pts[1], pts[2], p) as i64;
                let w1 = edge(pts[2], pts[0], p) as i64;
                let w2 = edge(pts[0], pts[1], p) as i64;
                if (w0 >= 0 && w1 >= 0 && w2 >= 0) || (w0 <= 0 && w1 <= 0 && w2 <= 0) {
                    let u = interp_uv(uvs[0].0, uvs[1].0, uvs[2].0, w0, w1, w2, area64);
                    let v = interp_uv(uvs[0].1, uvs[1].1, uvs[2].1, w0, w1, w2, area64);
                    if let Some(tex) = self.sample_texture(u, v, clut_x, clut_y) {
                        self.write_draw_pixel(x, y, modulate(tex, mod_color));
                    }
                }
            }
        }
    }

    fn draw_gouraud_textured_triangle(
        &mut self,
        colors: [u32; 3],
        points: [(i32, i32); 3],
        uvs: [(u8, u8); 3],
        clut_x: u16,
        clut_y: u16,
    ) {
        let pts = points.map(|(x, y)| (x + self.drawing_offset_x, y + self.drawing_offset_y));
        let min_x = pts.iter().map(|(x, _)| *x).min().unwrap().max(self.drawing_area.left);
        let max_x = pts.iter().map(|(x, _)| *x).max().unwrap().min(self.drawing_area.right);
        let min_y = pts.iter().map(|(_, y)| *y).min().unwrap().max(self.drawing_area.top);
        let max_y = pts.iter().map(|(_, y)| *y).max().unwrap().min(self.drawing_area.bottom);

        let area = edge(pts[0], pts[1], pts[2]);
        if area == 0 {
            return;
        }
        let area64 = area as i64;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let p = (x, y);
                let w0 = edge(pts[1], pts[2], p) as i64;
                let w1 = edge(pts[2], pts[0], p) as i64;
                let w2 = edge(pts[0], pts[1], p) as i64;
                if (w0 >= 0 && w1 >= 0 && w2 >= 0) || (w0 <= 0 && w1 <= 0 && w2 <= 0) {
                    let u = interp_uv(uvs[0].0, uvs[1].0, uvs[2].0, w0, w1, w2, area64);
                    let v = interp_uv(uvs[0].1, uvs[1].1, uvs[2].1, w0, w1, w2, area64);
                    if let Some(tex) = self.sample_texture(u, v, clut_x, clut_y) {
                        let r = interp8(colors, w0, w1, w2, area64, 0);
                        let g = interp8(colors, w0, w1, w2, area64, 8);
                        let b = interp8(colors, w0, w1, w2, area64, 16);
                        self.write_draw_pixel(x, y, modulate(tex, r | (g << 8) | (b << 16)));
                    }
                }
            }
        }
    }

    fn draw_textured_rect(
        &mut self,
        mod_color: u32,
        x: i32,
        y: i32,
        u0: u8,
        v0: u8,
        width: u32,
        height: u32,
        clut_x: u16,
        clut_y: u16,
    ) {
        let ox = x + self.drawing_offset_x;
        let oy = y + self.drawing_offset_y;
        for dy in 0..height as i32 {
            for dx in 0..width as i32 {
                let u = u0.wrapping_add(dx as u8);
                let v = v0.wrapping_add(dy as u8);
                if let Some(tex) = self.sample_texture(u, v, clut_x, clut_y) {
                    self.write_draw_pixel(ox + dx, oy + dy, modulate(tex, mod_color));
                }
            }
        }
    }

    fn fill_rect(&mut self, x: i32, y: i32, width: u32, height: u32, color: u16) {
        let x = x + self.drawing_offset_x;
        let y = y + self.drawing_offset_y;
        for yy in y..y + height as i32 {
            for xx in x..x + width as i32 {
                self.write_draw_pixel(xx, yy, color);
            }
        }
    }

    fn fill_rect_raw(&mut self, x: u32, y: u32, width: u32, height: u32, color: u16) {
        for yy in y..y + height {
            for xx in x..x + width {
                self.write_vram(xx as usize, yy as usize, color);
            }
        }
    }

    fn copy_vram(
        &mut self,
        src_x: usize,
        src_y: usize,
        dst_x: usize,
        dst_y: usize,
        width: usize,
        height: usize,
    ) {
        // Read into temp buffer to handle overlapping regions correctly.
        let mut buf = vec![0u16; width * height];
        for y in 0..height {
            for x in 0..width {
                let sx = (src_x + x) % VRAM_WIDTH;
                let sy = (src_y + y) % VRAM_HEIGHT;
                buf[y * width + x] = self.vram[sy * VRAM_WIDTH + sx];
            }
        }
        for y in 0..height {
            for x in 0..width {
                let dx = (dst_x + x) % VRAM_WIDTH;
                let dy = (dst_y + y) % VRAM_HEIGHT;
                self.vram[dy * VRAM_WIDTH + dx] = buf[y * width + x];
            }
        }
    }

    // ---- Texture sampling ----

    /// Returns None for transparent texels (index 0 in CLUT modes).
    fn sample_texture(&self, u: u8, v: u8, clut_x: u16, clut_y: u16) -> Option<u16> {
        let tx = self.tpage.x_base as usize;
        let ty = self.tpage.y_base as usize;
        match self.tpage.depth {
            0 => {
                // 4-bit CLUT: 4 texels per 16-bit VRAM word
                let vx = tx + u as usize / 4;
                let vy = ty + v as usize;
                let word = self.read_vram(vx, vy);
                let nibble = ((word >> ((u as usize % 4) * 4)) & 0xf) as usize;
                if nibble == 0 {
                    return None;
                }
                Some(self.read_vram(clut_x as usize + nibble, clut_y as usize))
            }
            1 => {
                // 8-bit CLUT: 2 texels per 16-bit VRAM word
                let vx = tx + u as usize / 2;
                let vy = ty + v as usize;
                let word = self.read_vram(vx, vy);
                let byte = ((word >> ((u as usize % 2) * 8)) & 0xff) as usize;
                if byte == 0 {
                    return None;
                }
                Some(self.read_vram(clut_x as usize + byte, clut_y as usize))
            }
            _ => {
                // 15-bit direct: 1 texel per 16-bit VRAM word
                Some(self.read_vram(tx + u as usize, ty + v as usize))
            }
        }
    }

    // ---- Pixel / VRAM writes ----

    fn write_draw_pixel(&mut self, x: i32, y: i32, color: u16) {
        if x < self.drawing_area.left
            || x > self.drawing_area.right
            || y < self.drawing_area.top
            || y > self.drawing_area.bottom
        {
            return;
        }
        if x >= 0 && y >= 0 {
            self.write_vram(x as usize, y as usize, color);
        }
    }

    fn write_vram(&mut self, x: usize, y: usize, color: u16) {
        if x < VRAM_WIDTH && y < VRAM_HEIGHT {
            self.vram[y * VRAM_WIDTH + x] = color;
        }
    }

    fn read_vram(&self, x: usize, y: usize) -> u16 {
        if x < VRAM_WIDTH && y < VRAM_HEIGHT {
            self.vram[y * VRAM_WIDTH + x]
        } else {
            0
        }
    }
}

impl Default for Gpu {
    fn default() -> Self {
        Self::new()
    }
}

// ---- Free helpers ----

fn xy(word: u32) -> (u32, u32) {
    (word & 0xffff, (word >> 16) & 0xffff)
}

fn vertex(word: u32) -> (i32, i32) {
    (
        sign_extend(word & 0x7ff, 11),
        sign_extend((word >> 16) & 0x7ff, 11),
    )
}

fn parse_uv(word: u32) -> (u8, u8) {
    ((word & 0xff) as u8, ((word >> 8) & 0xff) as u8)
}

fn sign_extend(value: u32, bits: u32) -> i32 {
    let shift = 32 - bits;
    ((value << shift) as i32) >> shift
}

fn color24_to_rgb555(word: u32) -> u16 {
    let r = (word & 0xff) as u16 >> 3;
    let g = ((word >> 8) & 0xff) as u16 >> 3;
    let b = ((word >> 16) & 0xff) as u16 >> 3;
    r | (g << 5) | (b << 10)
}

fn rgb555_to_rgb888(pixel: u16) -> [u8; 3] {
    let r = (pixel & 0x1f) as u8;
    let g = ((pixel >> 5) & 0x1f) as u8;
    let b = ((pixel >> 10) & 0x1f) as u8;
    [
        (r << 3) | (r >> 2),
        (g << 3) | (g >> 2),
        (b << 3) | (b >> 2),
    ]
}

fn edge(a: (i32, i32), b: (i32, i32), c: (i32, i32)) -> i32 {
    (c.0 - a.0) * (b.1 - a.1) - (c.1 - a.1) * (b.0 - a.0)
}

/// Barycentrically interpolate an 8-bit color channel across a triangle.
/// w0/w1/w2 are the barycentric weights (signed), area64 is the signed triangle area.
/// All weights and area have the same sign for interior points.
fn interp8(colors: [u32; 3], w0: i64, w1: i64, w2: i64, area64: i64, shift: u32) -> u32 {
    let c0 = ((colors[0] >> shift) & 0xff) as i64;
    let c1 = ((colors[1] >> shift) & 0xff) as i64;
    let c2 = ((colors[2] >> shift) & 0xff) as i64;
    ((c0 * w0 + c1 * w1 + c2 * w2) / area64).clamp(0, 255) as u32
}

/// Barycentrically interpolate an 8-bit UV coordinate.
fn interp_uv(u0: u8, u1: u8, u2: u8, w0: i64, w1: i64, w2: i64, area64: i64) -> u8 {
    let r = ((u0 as i64 * w0 + u1 as i64 * w1 + u2 as i64 * w2) / area64).clamp(0, 255) as u8;
    r
}

/// Modulate a 15-bit texture color by a 24-bit RGB vertex color.
/// Formula: out_channel = (tex_5bit * mod_8bit) / 128, clamped to 0-31.
fn modulate(tex: u16, color: u32) -> u16 {
    let tr = (tex & 0x1f) as u32;
    let tg = ((tex >> 5) & 0x1f) as u32;
    let tb = ((tex >> 10) & 0x1f) as u32;
    let r = ((tr * (color & 0xff)) >> 7).min(31) as u16;
    let g = ((tg * ((color >> 8) & 0xff)) >> 7).min(31) as u16;
    let b = ((tb * ((color >> 16) & 0xff)) >> 7).min(31) as u16;
    r | (g << 5) | (b << 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_vram_rectangle() {
        let mut gpu = Gpu::new();

        gpu.write_gp0(0x02_00_00_ff);
        gpu.write_gp0(10 | (20 << 16));
        gpu.write_gp0(2 | (3 << 16));

        let rgb = gpu.framebuffer_rgb();
        let w = gpu.display_width();
        let offset = (20 * w + 10) * 3;

        assert_eq!(&rgb[offset..offset + 3], &[0xff, 0x00, 0x00]);
    }

    #[test]
    fn uploads_cpu_image_to_vram() {
        let mut gpu = Gpu::new();

        gpu.write_gp0(0xa0_00_00_00);
        gpu.write_gp0(4 | (5 << 16));
        gpu.write_gp0(2 | (1 << 16));
        gpu.write_gp0(0x001f_7c00);

        let rgb = gpu.framebuffer_rgb();
        let w = gpu.display_width();
        let first = (5 * w + 4) * 3;
        let second = (5 * w + 5) * 3;

        assert_eq!(&rgb[first..first + 3], &[0x00, 0x00, 0xff]);
        assert_eq!(&rgb[second..second + 3], &[0xff, 0x00, 0x00]);
    }

    #[test]
    fn gp1_reset_does_not_clear_vram() {
        let mut gpu = Gpu::new();

        // Draw a pixel via FillRectangle
        gpu.write_gp0(0x02_00_ff_00); // green
        gpu.write_gp0(5 | (5 << 16));
        gpu.write_gp0(1 | (1 << 16));

        // GP1(00) soft reset
        gpu.write_gp1(0x00_00_00_00);

        // Pixel should still be there in VRAM
        let rgb = gpu.framebuffer_rgb();
        let w = gpu.display_width();
        let offset = (5 * w + 5) * 3;
        assert_eq!(&rgb[offset..offset + 3], &[0x00, 0xff, 0x00]);
    }

    #[test]
    fn rect_opcode_decode_textured_variable() {
        // 0x64 = variable-size textured rect: should need 4 words
        let cmd = Gp0Command::from_word(0x64_00_00_00);
        assert_eq!(cmd, Gp0Command::Rect(0x64));
        assert_eq!(cmd.word_count(), 4);
    }

    #[test]
    fn rect_opcode_decode_mono_8x8() {
        // 0x70 = 8x8 mono rect: should need 2 words
        let cmd = Gp0Command::from_word(0x70_00_00_00);
        assert_eq!(cmd, Gp0Command::Rect(0x70));
        assert_eq!(cmd.word_count(), 2);
    }
}
