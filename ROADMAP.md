# Roadmap

CopperForge is an evolving platform, and we have exciting plans for its future. Below is our current roadmap, which outlines completed features, ongoing development, and planned enhancements.


## Current Status
CopperForge is actively being developed with a focus on providing a comprehensive PCB design and manufacturing preparation tool. The platform is built using Rust and leverages the `egui` framework for a modern user interface. The application is designed to be modular and extensible, allowing for future enhancements and integrations with other tools and services.

One of the big goals is to be able to have an solid **plugin system** that allows for easy integration of new features and tools, making CopperForge a versatile platform for PCB design and manufacturing. 

For example, a plugin could be developed to support DRC functions on the Gerber files, or to provide additional manufacturing optimization algorithms. This would allow for a more flexible and extensible platform that can adapt to the needs of the community and the industry.

At present, the preferred approach of a plugin is via wasm, but we are also looking into a more traditional plugin system that allows for Rust code to be compiled and run within the CopperForge application. The advantage of wasm is it's overall portability, but the disadvantage is that it requires a bit more work to get up and running. The traditional plugin system would allow for easier integration of new features, but would require more work to maintain compatibility with the CopperForge application.

### Completed ✅
- Direct KiCad PCB file import
- Multi-layer Gerber visualization
- Interactive measurement tools
- Zoom and pan controls
- Interactive layer controls
- **Real-time BOM generation with KiCad IPC integration**

### In Development 🚧
- Support for LibrePCB import via the command line interface
- Enhanced DRC with customizable rule sets
- Manufacturing optimization algorithms
- Advanced BOM export formats (CSV, Excel, JSON)

### Planned 📋
- PCB Fabrication and Assembly house specific workflows
- Integration with popular PCB fabrication services (e.g., JLCPCB, PCBWay)
- Manufacturing cost estimation
- Component library validation
- Advanced panelization tools
- Assembly preparation features
- Integration with external CAM tools