//! Regenerate the phon `Message` wire module consumed by `@bearcove/vox-wire`.
//! Run `cargo test -p vox-codegen --test phon_wire_module` after changing the
//! `Message` envelope; the conformance test in vox-wire (phon_decode.test.ts)
//! validates that phon-ts decodes every golden wire vector against it.

#[test]
fn regenerate_phon_wire_module() {
    let ts = vox_codegen::targets::typescript::generate_phon_wire();
    let out = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../typescript/packages/vox-wire/src/wire.phon.generated.ts"
    );
    std::fs::write(out, &ts).expect("write wire module");
    assert!(ts.contains("export const registry"));
    assert!(ts.contains("export const schemaId"));
    assert!(ts.contains("export interface RequestCall"));
}
