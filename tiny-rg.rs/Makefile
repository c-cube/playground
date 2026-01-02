
build:
	cargo build --release

bench: build
	hyperfine --warmup=1 \
		"./target/release/tiny-rg --par '.*let.*' ~/w/zipperposition/" \
		"./target/release/tiny-rg '.*let.*' ~/w/zipperposition/"
