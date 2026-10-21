lint:
    prek run --all-files

test:
    cargo test

coverage:
    cargo tarpaulin --target-dir target/coverage --skip-clean
    xdg-open tarpaulin-report.html
