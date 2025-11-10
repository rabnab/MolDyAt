This is a Rust nightly project. Use the toolchain from rust-toolchain (nightly) when building and testing.
Tests: add unit/integration tests per module in a separate file next to the module (e.g., for src/foo.rs put tests in tests/foo_tests.rs or src/foo_tests.rs per your layout).
Before committing always run rustfmt (cargo fmt) and ensure code is formatted.
Optional: add a CI check that runs cargo +nightly fmt -- --check and cargo +nightly test.