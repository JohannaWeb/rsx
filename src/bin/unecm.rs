use std::env;
use std::path::PathBuf;

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> ps1_emulator::Result<()> {
    let mut args = env::args_os().skip(1);
    let input = match args.next() {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("usage: unecm <input.ecm> [output.bin]");
            return Ok(());
        }
    };
    let output = args.next().map(PathBuf::from).unwrap_or_else(|| {
        let mut path = input.clone();
        path.set_extension("");
        path
    });

    let bytes = ps1_emulator::decode_ecm_file(&input, &output)?;
    println!("decoded {} bytes to {}", bytes, output.display());
    Ok(())
}
