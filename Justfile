# Justfile for CopperForge Project Management

# Default recipe to display available commands
default:
    @just --list

# Build CopperForge in release mode
build:
    cargo build --release

# Run CopperForge
run:
    cargo run --release

# Setup KiCad libraries (KiVerse + Atlantix-EDA resistors)
setup-libraries:
    #!/usr/bin/env bash
    set -euo pipefail

    LIBS_DIR="${HOME}/.kicad_libs"
    echo "Setting up KiCad libraries in ${LIBS_DIR}"

    # Create libraries directory if it doesn't exist
    mkdir -p "${LIBS_DIR}"

    # Clone or update KiVerse
    if [ -d "${LIBS_DIR}/kiverse" ]; then
        echo "Updating KiVerse..."
        cd "${LIBS_DIR}/kiverse"
        git pull
    else
        echo "Cloning KiVerse..."
        git clone https://github.com/saturn77/KiVerse.git "${LIBS_DIR}/kiverse"
    fi

    echo "✅ KiVerse library setup complete at ${LIBS_DIR}/kiverse"
    echo "📦 Symbols: ${LIBS_DIR}/kiverse/symbols"
    echo "📦 Footprints: ${LIBS_DIR}/kiverse/footprints"

# Upload Atlantix-EDA resistors to KiVerse (for maintainers)
upload-atlantix-resistors ATLANTIX_EDA_PATH:
    #!/usr/bin/env bash
    set -euo pipefail

    LIBS_DIR="${HOME}/.kicad_libs"
    KIVERSE_DIR="${LIBS_DIR}/kiverse"
    ATLANTIX_DIR="{{ATLANTIX_EDA_PATH}}"

    if [ ! -d "${KIVERSE_DIR}" ]; then
        echo "❌ KiVerse not found. Run 'just setup-libraries' first."
        exit 1
    fi

    if [ ! -d "${ATLANTIX_DIR}" ]; then
        echo "❌ Atlantix-EDA path not found: ${ATLANTIX_DIR}"
        exit 1
    fi

    echo "📤 Uploading Atlantix-EDA resistor libraries to KiVerse..."

    # Create atlantix-eda directories in KiVerse
    mkdir -p "${KIVERSE_DIR}/symbols/atlantix-eda"
    mkdir -p "${KIVERSE_DIR}/footprints/atlantix-eda"

    # Copy symbol files
    if [ -d "${ATLANTIX_DIR}/outputs/kicad/symbols" ]; then
        echo "Copying symbol files..."
        cp -v "${ATLANTIX_DIR}"/outputs/kicad/symbols/Atlantix_R_*.kicad_sym \
           "${KIVERSE_DIR}/symbols/atlantix-eda/" 2>/dev/null || echo "No symbol files found"
    fi

    # Copy footprint files
    if [ -d "${ATLANTIX_DIR}/outputs/kicad/Atlantix_Resistors.pretty" ]; then
        echo "Copying footprint files..."
        cp -rv "${ATLANTIX_DIR}/outputs/kicad/Atlantix_Resistors.pretty" \
           "${KIVERSE_DIR}/footprints/atlantix-eda/" 2>/dev/null || echo "No footprint files found"
    fi

    echo "✅ Upload complete!"
    echo ""
    echo "Next steps:"
    echo "1. cd ${KIVERSE_DIR}"
    echo "2. git status"
    echo "3. git add symbols/atlantix-eda/ footprints/atlantix-eda/"
    echo "4. git commit -m \"Add Atlantix-EDA resistor libraries\""
    echo "5. git push"

# Generate Atlantix-EDA resistor libraries
generate-resistors ATLANTIX_EDA_PATH:
    #!/usr/bin/env bash
    set -euo pipefail

    ATLANTIX_DIR="{{ATLANTIX_EDA_PATH}}"

    if [ ! -d "${ATLANTIX_DIR}" ]; then
        echo "❌ Atlantix-EDA path not found: ${ATLANTIX_DIR}"
        exit 1
    fi

    echo "🔧 Generating Atlantix-EDA resistor libraries..."
    cd "${ATLANTIX_DIR}"
    cargo run --example gen_resistor --release -- \
        --format kicad \
        --packages "0402,0603,0805,1206,1210,2512" \
        --output-dir outputs

    echo "✅ Resistor libraries generated in ${ATLANTIX_DIR}/outputs"

# Clean build artifacts
clean:
    cargo clean

# Run tests
test:
    cargo test

# Format code
fmt:
    cargo fmt

# Check code with clippy
clippy:
    cargo clippy -- -D warnings

# Full workflow: generate resistors, upload to KiVerse, setup libraries
full-setup ATLANTIX_EDA_PATH:
    just generate-resistors {{ATLANTIX_EDA_PATH}}
    just upload-atlantix-resistors {{ATLANTIX_EDA_PATH}}
    just setup-libraries

# Show library paths
show-lib-paths:
    #!/usr/bin/env bash
    LIBS_DIR="${HOME}/.kicad_libs"
    echo "KiCad Libraries Directory: ${LIBS_DIR}"
    echo ""
    if [ -d "${LIBS_DIR}/kiverse" ]; then
        echo "✅ KiVerse: ${LIBS_DIR}/kiverse"
        if [ -d "${LIBS_DIR}/kiverse/symbols/atlantix-eda" ]; then
            echo "✅ Atlantix Resistors: ${LIBS_DIR}/kiverse/symbols/atlantix-eda"
        else
            echo "❌ Atlantix Resistors: Not found"
        fi
    else
        echo "❌ KiVerse: Not installed"
        echo "   Run: just setup-libraries"
    fi
