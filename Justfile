# Justfile for CopperForge Project Management

# The cuforge-services backend repo lives beside this one.
services_dir := "../cuforge-services"

# Default recipe to display available commands
default:
    @just --list

# Build CopperForge in release mode
build:
    cargo build --release

# Run CopperForge (starts cuforge-services first if it isn't already up)
run: start-services
    cargo run --release

# Run ONLY the CopperForge app (no backend startup)
ui:
    cargo run --release

# Runs from the services repo root — the server's ./copperforge.db and
# ./data/releases paths are relative to its cwd. Logs to
# ../cuforge-services/cuforge-services.log.
# Start cuforge-services in the background unless :8421 already answers
start-services:
    #!/usr/bin/env bash
    set -euo pipefail
    if curl -sf -m 2 http://127.0.0.1:8421/health >/dev/null 2>&1; then
        echo "✅ cuforge-services already running on 127.0.0.1:8421"
        exit 0
    fi
    echo "🚀 Starting cuforge-services (127.0.0.1:8421)..."
    cd {{services_dir}}
    cargo build --release
    nohup ./target/release/cuforge-services >> cuforge-services.log 2>&1 &
    echo $! > cuforge-services.pid
    # Wait for /health so the app's status badge is green from the start.
    for _ in $(seq 1 30); do
        if curl -sf -m 2 http://127.0.0.1:8421/health >/dev/null 2>&1; then
            echo "✅ cuforge-services is up (pid $(cat cuforge-services.pid))"
            exit 0
        fi
        sleep 0.5
    done
    echo "⚠️  cuforge-services didn't answer /health within 15s — check {{services_dir}}/cuforge-services.log"

# Stop a cuforge-services instance started by `just start-services`.
stop-services:
    #!/usr/bin/env bash
    set -euo pipefail
    pidfile="{{services_dir}}/cuforge-services.pid"
    if [ -f "${pidfile}" ] && kill "$(cat "${pidfile}")" 2>/dev/null; then
        echo "🛑 Stopped cuforge-services (pid $(cat "${pidfile}"))"
        rm -f "${pidfile}"
    else
        echo "No cuforge-services pidfile (or process already gone)."
        rm -f "${pidfile}"
    fi

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

# Install a desktop launcher (Linux Mint / Cinnamon): menu entry + icon.
# Points at the cargo-installed binary; re-run after `cargo install` if the
# binary location ever changes. Safe to re-run — it overwrites in place.
install-launcher:
    #!/usr/bin/env bash
    set -euo pipefail
    BIN="${HOME}/.cargo/bin/copperforge"
    if [ ! -x "${BIN}" ]; then
        echo "❌ ${BIN} not found — run 'cargo install --path . --force' first"
        exit 1
    fi
    install -Dm644 assets/media/copperforge-icon.png \
        "${HOME}/.local/share/icons/hicolor/128x128/apps/copperforge.png"
    mkdir -p "${HOME}/.local/share/applications"
    printf '%s\n' \
        '[Desktop Entry]' \
        'Type=Application' \
        'Name=CopperForge' \
        'Comment=Design automation and rule-driven layout tooling for KiCad' \
        "Exec=${BIN}" \
        "Icon=${HOME}/.local/share/icons/hicolor/128x128/apps/copperforge.png" \
        'Terminal=false' \
        'Categories=Development;Electronics;Engineering;' \
        'StartupWMClass=copperforge' \
        > "${HOME}/.local/share/applications/copperforge.desktop"
    update-desktop-database "${HOME}/.local/share/applications" 2>/dev/null || true
    gtk-update-icon-cache -f "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
    echo "✅ Launcher installed — CopperForge is in your applications menu."

# Remove the desktop launcher and icon
uninstall-launcher:
    rm -f "${HOME}/.local/share/applications/copperforge.desktop" \
          "${HOME}/.local/share/icons/hicolor/128x128/apps/copperforge.png"
    -update-desktop-database "${HOME}/.local/share/applications" 2>/dev/null
    @echo "🗑  Launcher removed."
