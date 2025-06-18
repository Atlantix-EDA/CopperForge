# KiForge ECS 

The Entity-Component-System for KiForge is an approach to modularize the 
handling of electrical and mechanical components, as well as board level
entities such as traces, copper, vias, mounting holes, fiducials, and
silkscreen. It's debateable whether to call a trace or copper pour a component, but entities seems appropriate. 


## Functionality

The decision to use an ECS architecture allows for a flexible and extensible design, where components can be added or modified without affecting the overall system. This modularity is particularly useful for handling complex PCB designs and mechanical assemblies, enabling the separation of concerns between different aspects of the design.

Also after testing a 3d layout engine in Bevy itself, the ECS approach within egui allows for a more responsive and interactive user interface, as it can handle real-time updates to the design without the overhead of a full game engine. 

Another benefit is the much faster compile times compared to the Bevy ECS, which can be significant when working with large and complex designs. 

# Modules
The ECS is divided into several modules, each responsible for a specific aspect of the design. This modular approach allows for easier maintenance and scalability of the codebase.

An overall summary of the modules in the design: 

- `mesh3d.rs` - Generates 3D meshes from 2D polygons, handles extrusion and stackup generation.
- `pcb.rs` - Manages PCB components, including traces, vias, and other board-level features.
- `components.rs` - Defines the core components used in the ECS, such as `Mesh3D`, `Polygon2D`, and utility functions.
- `compatibility.rs` - Provides compatibility layers for different versions of the ECS, ensuring smooth transitions and updates.