# New KiCad Project Creation Feature

## Overview

CopperForge now supports creating new KiCad projects from scratch! This feature allows you to:

1. Create a new KiCad project with all necessary files (.kicad_pro, .kicad_sch, .kicad_pcb)
2. Automatically configure symbol and footprint libraries
3. Include KiVerse and Atlantix-EDA resistor libraries
4. Add project metadata (author, company, description)
5. Manage projects in CopperForge's project database

## Quick Start

### 1. Setup Libraries (One-Time)

Before creating your first project, setup the KiCad libraries:

```bash
# Install 'just' command runner if you don't have it
cargo install just

# Setup KiVerse library
just setup-libraries
```

This will clone KiVerse to `~/kiverse`.

### 2. Create a New Project

1. Launch CopperForge: `just run` or `cargo run --release`
2. Navigate to the **Project Manager** tab
3. Click **"➕ New Project"**
4. Select **"🆕 Create New KiCad Project"**
5. Fill in the project details:
   - **Project Name**: Your project name
   - **Description**: Brief description
   - **Location**: Where to create the project directory
   - **Author**: Your name
   - **Company**: Your company/organization
6. Choose library options:
   - ☑️ Include KiVerse Symbol Library
   - ☑️ Include Atlantix-EDA Resistor Library
7. Click **"Create"**

## What Gets Created

When you create a new project named "MotorController", CopperForge generates:

```
MotorController/
├── MotorController.kicad_pro    # Project file
├── MotorController.kicad_sch    # Schematic file
├── MotorController.kicad_pcb    # PCB layout file
├── sym-lib-table                # Symbol library configuration
├── fp-lib-table                 # Footprint library configuration
└── README.md                    # Project documentation
```

### Symbol Library Table (sym-lib-table)

Automatically includes:
- **KiVerse** symbols
- **Atlantix Resistor** libraries for packages: 0402, 0603, 0805, 1206, 1210, 2512

### Footprint Library Table (fp-lib-table)

Automatically includes:
- **KiVerse** footprints
- **Atlantix Resistor** footprints

### Project Metadata

The `.kicad_pro` file includes text variables:
- `AUTHOR`: Your name
- `COMPANY`: Your company
- `DATE`: Creation date
- `DESCRIPTION`: Project description

## Library Management

### Using KiVerse + Atlantix Libraries

The generated `sym-lib-table` and `fp-lib-table` reference:

```
${HOME}/kiverse/symbols/...
${HOME}/kiverse/footprints/...
```

### Adding Atlantix Resistors to KiVerse

If you want to contribute Atlantix-EDA resistor libraries to KiVerse:

```bash
# 1. Generate the resistor libraries
just generate-resistors /path/to/atlantix-eda

# 2. Upload to your KiVerse clone
just upload-atlantix-resistors /path/to/atlantix-eda

# 3. Follow the git instructions to commit and push
```

## Justfile Commands

The `Justfile` provides convenient commands:

```bash
# Setup libraries
just setup-libraries

# Generate Atlantix resistor libraries
just generate-resistors /path/to/atlantix-eda

# Upload resistors to KiVerse
just upload-atlantix-resistors /path/to/atlantix-eda

# Full workflow (generate + upload + setup)
just full-setup /path/to/atlantix-eda

# Show library paths
just show-lib-paths

# Build and run
just build
just run
```

## Importing Existing Projects

You can still import existing KiCad PCB files:

1. Click **"➕ New Project"**
2. Select **"📂 Import Existing PCB"**
3. Browse to your `.kicad_pcb` file
4. Fill in project details
5. Click **"Create"**

## Architecture

### File Structure

```
crates/copperforge-core/src/
├── project_manager/
│   ├── mod.rs                    # ProjectManagerState
│   ├── kicad_project.rs          # New! KiCad project generation
│   ├── database.rs               # Project database
│   └── bom.rs                    # BOM components
└── ui/
    └── project_manager_panel.rs  # Updated UI with new dialog
```

### Key Components

1. **`kicad_project.rs`**:
   - `NewKicadProjectInfo` struct
   - `create_kicad_project()` function
   - Generates all KiCad project files

2. **`ProjectManagerState`**:
   - New fields for project creation
   - `create_new_kicad_project_from_scratch()` method

3. **`project_manager_panel.rs`**:
   - Enhanced create dialog UI
   - Radio button to toggle between new/import
   - Library configuration options

## Workflow Integration

### Typical Workflow

1. **Create Project in CopperForge**
   ```bash
   just run
   # Use GUI to create project
   ```

2. **Open in KiCad**
   ```bash
   kicad ~/path/to/MotorController/MotorController.kicad_pro
   ```

3. **Design Your PCB**
   - Draw schematic
   - Use KiVerse and Atlantix resistor symbols
   - Layout PCB

4. **Back to CopperForge**
   - Generate gerbers
   - View in gerber viewer
   - Export BOM
   - Run DRC

### Libraries in KiCad

When you open your project in KiCad, you'll see the libraries available:

**Symbol Editor:**
- KiVerse
- Atlantix_R_0402
- Atlantix_R_0603
- Atlantix_R_0805
- Atlantix_R_1206
- Atlantix_R_1210
- Atlantix_R_2512

**Footprint Editor:**
- KiVerse.pretty
- Atlantix_Resistors.pretty

## Configuration

### Custom Library Paths

Edit the library path in the create dialog or modify after creation:

```
Default: ${HOME}/kiverse
Custom: /path/to/your/libraries
```

### Project Location

Projects are created in the directory you specify. The project name becomes a subdirectory:

```
Location: ~/projects
Name: MotorController
Result: ~/projects/MotorController/
```

## Troubleshooting

### Libraries Not Found in KiCad

If KiCad shows "Library not found" errors:

1. Check library path:
   ```bash
   just show-lib-paths
   ```

2. Ensure KiVerse is cloned:
   ```bash
   just setup-libraries
   ```

3. Verify the path in `sym-lib-table` and `fp-lib-table` matches where KiVerse is installed

### Permission Errors

If you get permission errors creating projects:

1. Check write permissions on the target directory
2. Try a different location (e.g., `~/Documents` instead of `/opt`)

### Atlantix Resistors Not Available

The Atlantix resistor libraries need to be in KiVerse. Two options:

1. **Wait for upstream**: Once merged into KiVerse main repo
2. **Manual setup**: Use `just upload-atlantix-resistors` to add them locally

## Future Enhancements

Planned features:
- [ ] Project templates (motor control, power supply, etc.)
- [ ] Custom library selection dialog
- [ ] Git repository initialization
- [ ] Component library validation
- [ ] Multi-board projects
- [ ] Integration with fabrication houses

## Contributing

To contribute Atlantix-EDA libraries to KiVerse:

1. Fork KiVerse: https://github.com/saturn77/KiVerse
2. Generate libraries: `just generate-resistors /path/to/atlantix-eda`
3. Add to your fork: `just upload-atlantix-resistors /path/to/atlantix-eda`
4. Create pull request to KiVerse upstream

## License

This feature is part of CopperForge and licensed under MIT.

---

**Created with CopperForge v0.1.7** - PCB & CAM for KiCad
