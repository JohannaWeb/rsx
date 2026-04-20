CARGO ?= cargo
SEVENZ ?= 7z

ROOT := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

BIOS ?= $(ROOT)SCPH1001.BIN
GAME ?= doom
STEPS ?= 20000
FRAME ?= frame.ppm
BOOT ?= --bios-boot

DOOM_ARCHIVE := $(ROOT)Doom.7z
DOOM_CUE := $(ROOT)Doom.cue
DOOM_TRACK := $(ROOT)Doom (Track 1).bin

ACE_ARCHIVE := $(ROOT)Ace Combat 2 (USA).7z
ACE_CUE := $(ROOT)Ace Combat 2 (USA).cue
ACE_BIN := $(ROOT)Ace Combat 2 (USA).bin

.PHONY: all build check prepare prepare-doom prepare-ace run run-doom run-ace window window-doom window-ace dump-frame clean-games

all: build

build:
	$(CARGO) build

check:
	$(CARGO) check

prepare: prepare-$(GAME)

prepare-doom:
	@test -f "$(DOOM_CUE)" -a -f "$(DOOM_TRACK)" || $(SEVENZ) x -y "$(DOOM_ARCHIVE)"

prepare-ace:
	@test -f "$(ACE_CUE)" -a -f "$(ACE_BIN)" || $(SEVENZ) x -y "$(ACE_ARCHIVE)"

run: run-$(GAME)

run-doom: prepare-doom
	$(CARGO) run -- "$(BIOS)" "$(DOOM_CUE)" "$(STEPS)" $(BOOT)

run-ace: prepare-ace
	$(CARGO) run -- "$(BIOS)" "$(ACE_CUE)" "$(STEPS)" $(BOOT)

window: window-$(GAME)

window-doom: prepare-doom
	$(CARGO) run -- "$(BIOS)" "$(DOOM_CUE)" --window

window-ace: prepare-ace
	$(CARGO) run -- "$(BIOS)" "$(ACE_CUE)" --window $(BOOT)

dump-frame: prepare-$(GAME)
	PS1_DUMP_FRAME="$(FRAME)" $(MAKE) run GAME="$(GAME)" STEPS="$(STEPS)"

clean-games:
	rm -f "$(DOOM_CUE)" "$(DOOM_TRACK)" "Doom (Track 2).bin" "Doom (Track 3).bin" "Doom (Track 4).bin" "Doom (Track 5).bin" "Doom (Track 6).bin" "Doom (Track 7).bin" "Doom (Track 8).bin" readme.html
	rm -f "$(ACE_CUE)" "$(ACE_BIN)"
