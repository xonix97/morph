.PHONY: test

CARGO ?= cargo
PYTHON ?= python3.11
CARGO_FLAGS = --locked --offline

test:
	RUSTUP_AUTO_INSTALL=0 CARGO_NET_OFFLINE=true $(CARGO) test $(CARGO_FLAGS)
	$(PYTHON) -m unittest discover -s tests -v
	RUSTUP_AUTO_INSTALL=0 CARGO_NET_OFFLINE=true $(CARGO) run $(CARGO_FLAGS) -- demo all
