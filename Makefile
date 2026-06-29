BIN_NAME := ghostteam
CARGO := cargo
INSTALL_DIR := /usr/local/bin
INSTALL_BIN := $(INSTALL_DIR)/$(BIN_NAME)

.PHONY: build release clippy-strict install uninstall

build:
	$(CARGO) build

release:
	$(CARGO) build --release

clippy-strict:
	$(CARGO) clippy-strict

install: release
	install -m 755 target/release/$(BIN_NAME) $(INSTALL_BIN)

uninstall:
	rm -f $(INSTALL_BIN)
