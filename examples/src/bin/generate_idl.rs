/// Generate IDL JSON for the provenance program.
///
/// Usage:
///   cargo run --bin generate_idl > provenance-idl.json

spel_framework::generate_idl!("../methods/guest/src/bin/provenance.rs");
