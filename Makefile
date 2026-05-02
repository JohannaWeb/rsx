CARGO ?= cargo
SEVENZ ?= 7z

ROOT := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))
BIOS_DIR := $(ROOT)bios
GAMES_DIR := $(ROOT)games
ARTIFACTS_DIR := $(ROOT)artifacts

BIOS ?= $(BIOS_DIR)/SCPH1001.BIN
GAME ?= doom
STEPS ?= 20000
FRAME ?= $(ARTIFACTS_DIR)/frames/frame.ppm
BIOS_BOOT ?= --bios-boot

DOOM_ARCHIVE := $(GAMES_DIR)/Doom.7z
DOOM_CUE := $(GAMES_DIR)/Doom.cue
DOOM_TRACK := $(GAMES_DIR)/Doom (Track 1).bin

ACE_ARCHIVE := $(GAMES_DIR)/Ace Combat 2 (USA).7z
ACE_CUE := $(GAMES_DIR)/Ace Combat 2 (USA).cue
ACE_BIN := $(GAMES_DIR)/Ace Combat 2 (USA).bin

.PHONY: all build check prepare prepare-doom prepare-ace run run-doom run-ace window window-doom window-ace dump-frame clean-games

all: build

build:
	$(CARGO) build

check:
	$(CARGO) check

prepare: prepare-$(GAME)

prepare-doom:
	@mkdir -p "$(GAMES_DIR)"
	@test -f "$(DOOM_CUE)" -a -f "$(DOOM_TRACK)" || $(SEVENZ) x -y "$(DOOM_ARCHIVE)" -o"$(GAMES_DIR)"

prepare-ace:
	@mkdir -p "$(GAMES_DIR)"
	@test -f "$(ACE_CUE)" -a -f "$(ACE_BIN)" || $(SEVENZ) x -y "$(ACE_ARCHIVE)" -o"$(GAMES_DIR)"

run: run-$(GAME)

run-doom: prepare-doom
	$(CARGO) run -- "$(BIOS)" "$(DOOM_CUE)" "$(STEPS)"

run-ace: prepare-ace
	$(CARGO) run -- "$(BIOS)" "$(ACE_CUE)" "$(STEPS)"

window: window-$(GAME)

window-doom: prepare-doom
	$(CARGO) run -- "$(BIOS)" "$(DOOM_CUE)" --window

window-ace: prepare-ace
	$(CARGO) run -- "$(BIOS)" "$(ACE_CUE)" --window

run-doom-bios: prepare-doom
	$(CARGO) run -- "$(BIOS)" "$(DOOM_CUE)" "$(STEPS)" $(BIOS_BOOT)

run-ace-bios: prepare-ace
	$(CARGO) run -- "$(BIOS)" "$(ACE_CUE)" "$(STEPS)" $(BIOS_BOOT)

window-doom-bios: prepare-doom
	$(CARGO) run -- "$(BIOS)" "$(DOOM_CUE)" --window $(BIOS_BOOT)

window-ace-bios: prepare-ace
	$(CARGO) run -- "$(BIOS)" "$(ACE_CUE)" --window $(BIOS_BOOT)

dump-frame: prepare-$(GAME)
	@mkdir -p "$(ARTIFACTS_DIR)/frames"
	PS1_DUMP_FRAME="$(FRAME)" $(MAKE) run GAME="$(GAME)" STEPS="$(STEPS)"

clean-games:
	rm -f "$(DOOM_CUE)" "$(DOOM_TRACK)" "$(GAMES_DIR)/Doom (Track 2).bin" "$(GAMES_DIR)/Doom (Track 3).bin" "$(GAMES_DIR)/Doom (Track 4).bin" "$(GAMES_DIR)/Doom (Track 5).bin" "$(GAMES_DIR)/Doom (Track 6).bin" "$(GAMES_DIR)/Doom (Track 7).bin" "$(GAMES_DIR)/Doom (Track 8).bin" "$(GAMES_DIR)/readme.html"
	rm -f "$(ACE_CUE)" "$(ACE_BIN)"
