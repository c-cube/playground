
build:
	cargo build --release

bench: build
	hyperfine --warmup=1 \
		"./target/release/tiny-rg --par '\blet\b' ~/w/zipperposition/" \
		"./target/release/tiny-rg '\blet\b' ~/w/zipperposition/"
