#!/bin/bash
# Test script to verify global library setup

echo "Backing up current KiCad config..."
cp ~/.config/kicad/9.99/sym-lib-table ~/.config/kicad/9.99/sym-lib-table.backup.$(date +%s)
cp ~/.config/kicad/9.99/fp-lib-table ~/.config/kicad/9.99/fp-lib-table.backup.$(date +%s)

echo "Checking if KiVerse libraries are in global config before test..."
if grep -q "kiverse" ~/.config/kicad/9.99/sym-lib-table; then
    echo "  KiVerse symbols found in global config (already setup)"
else
    echo "  KiVerse symbols NOT found in global config"
fi

echo ""
echo "To test the new functionality:"
echo "1. Run CopperForge: cargo run --release"
echo "2. Create a new project with KiVerse enabled"
echo "3. The libraries will be automatically added to ~/.config/kicad/9.99/sym-lib-table and fp-lib-table"
echo "4. Open any KiCad project (even non-CopperForge ones) and verify KiVerse libraries are visible"
echo ""
echo "Or test programmatically with this Rust snippet:"
echo "  use copperforge_core::project_manager::kicad_global_libs::setup_kiverse_globally;"
echo "  let path = Some(PathBuf::from(\"~/.kicad_libs/kiverse\"));"
echo "  setup_kiverse_globally(path).unwrap();"
