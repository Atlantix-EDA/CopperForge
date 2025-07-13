# Contributing to kicad-ecs

Thank you for your interest in contributing! We value your time and want to make the contribution process as smooth as possible.

## Before You Start

### Scope

✅ **We welcome:**
- New ECS components for PCB analysis
- Additional analysis systems (thermal, mechanical, electrical, etc.)
- Performance improvements
- Documentation and examples
- Bug fixes
- Integration with other KiCad tools

❌ **Out of scope:**
- Changes to core ECS architecture without discussion
- Non-ECS approaches (this project is specifically about ECS)
- Features unrelated to PCB/KiCad

### Discussion First

For significant changes, please open an issue first to discuss:
- The problem you're solving
- Your proposed approach
- How it fits with the project's direction

This saves everyone time and frustration.

## Development Process

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Add tests if applicable
5. Run `cargo test` and `cargo clippy`
6. Commit with clear messages
7. Push and create a Pull Request

## Code Style

- Follow Rust standard style (`cargo fmt`)
- Keep functions focused and small
- Document public APIs
- Use descriptive variable names

## Example Contributions

### Adding a New Component Type

```rust
#[derive(Component, Debug, Clone)]
pub struct ThermalProperties {
    pub power_dissipation: f32,  // Watts
    pub thermal_resistance: f32,  // K/W
    pub max_temperature: f32,     // Celsius
}
```

### Adding a New System

```rust
pub fn thermal_analysis_system(
    query: Query<(&Position, &ThermalProperties)>
) {
    // Your analysis here
}
```

## Testing

- Unit tests for new components
- Integration tests for new systems
- Examples demonstrating real-world usage

## Documentation

- Document all public types and functions
- Include examples in doc comments
- Update README if adding major features

## Questions?

Open an issue or start a discussion. We're here to help!

## License

By contributing, you agree that your contributions will be licensed under the same terms as the project (MIT/Apache-2.0).