/// Zoom state machine for 3D camera
/// 
/// This is the SINGLE source of truth for zoom behavior.
/// All zoom operations go through this state machine.

#[derive(Debug, Clone)]
pub enum ZoomState {
    /// Normal state - zoom is stable
    Idle { factor: f32 },
    
    /// User is actively changing zoom (button press, scroll)
    UserChanging { from: f32, to: f32 },
    
    /// Smoothly transitioning to new zoom value
    Transitioning { from: f32, to: f32, progress: f32 },
    
    /// Zoom locked during camera operations (framing, presets)
    Locked { factor: f32 },
}

impl ZoomState {
    pub fn new(initial_zoom: f32) -> Self {
        ZoomState::Idle { 
            factor: initial_zoom.clamp(0.1, 100.0) 
        }
    }
    
    /// Get current effective zoom factor
    pub fn current_factor(&self) -> f32 {
        match self {
            ZoomState::Idle { factor } => *factor,
            ZoomState::UserChanging { to, .. } => *to,
            ZoomState::Transitioning { from, to, progress } => {
                lerp(*from, *to, *progress)
            }
            ZoomState::Locked { factor } => *factor,
        }
    }
    
    /// User wants to set zoom directly (zoom buttons)
    pub fn set_zoom(&mut self, new_zoom: f32) -> ZoomTransition {
        let clamped_zoom = new_zoom.clamp(0.1, 100.0);
        let current = self.current_factor();
        
        if (current - clamped_zoom).abs() < 0.001 {
            // No change needed
            return ZoomTransition::None;
        }
        
        match self {
            ZoomState::Locked { .. } => {
                // Can't change zoom while locked
                ZoomTransition::None
            }
            _ => {
                *self = ZoomState::UserChanging { 
                    from: current, 
                    to: clamped_zoom 
                };
                ZoomTransition::StartTransition
            }
        }
    }
    
    /// User is scrolling (relative zoom change)
    pub fn zoom_by_factor(&mut self, factor: f32) -> ZoomTransition {
        let current = self.current_factor();
        let new_zoom = (current * factor).clamp(0.1, 100.0);
        self.set_zoom(new_zoom)
    }
    
    /// Lock zoom during camera operations
    pub fn lock(&mut self) {
        let current = self.current_factor();
        *self = ZoomState::Locked { factor: current };
    }
    
    /// Unlock zoom after camera operations
    pub fn unlock(&mut self) {
        if let ZoomState::Locked { factor } = self {
            *self = ZoomState::Idle { factor: *factor };
        }
    }
    
    /// Update state machine (call every frame)
    pub fn update(&mut self, dt: f32) -> ZoomTransition {
        match self {
            ZoomState::UserChanging { from, to } => {
                // Start smooth transition
                *self = ZoomState::Transitioning { 
                    from: *from, 
                    to: *to, 
                    progress: 0.0 
                };
                ZoomTransition::Continue
            }
            ZoomState::Transitioning { from, to, progress } => {
                // Smooth transition with configurable speed
                let transition_speed = 8.0; // Adjust for faster/slower transitions
                *progress += dt * transition_speed;
                
                if *progress >= 1.0 {
                    // Transition complete
                    *self = ZoomState::Idle { factor: *to };
                    ZoomTransition::Complete
                } else {
                    ZoomTransition::Continue
                }
            }
            ZoomState::Idle { .. } | ZoomState::Locked { .. } => {
                ZoomTransition::None
            }
        }
    }
    
    /// Check if zoom can be changed
    pub fn can_change(&self) -> bool {
        !matches!(self, ZoomState::Locked { .. })
    }
}

#[derive(Debug, PartialEq)]
pub enum ZoomTransition {
    None,           // No action needed
    StartTransition, // Started a new transition
    Continue,       // Transition in progress
    Complete,       // Transition finished
}

/// Linear interpolation helper
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_zoom_state_basic() {
        let mut zoom = ZoomState::new(1.0);
        assert_eq!(zoom.current_factor(), 1.0);
        
        // Set new zoom
        let transition = zoom.set_zoom(5.0);
        assert_eq!(transition, ZoomTransition::StartTransition);
        assert_eq!(zoom.current_factor(), 5.0);
    }
    
    #[test]
    fn test_zoom_clamping() {
        let mut zoom = ZoomState::new(1.0);
        
        // Test upper clamp
        zoom.set_zoom(150.0);
        assert_eq!(zoom.current_factor(), 100.0);
        
        // Test lower clamp
        zoom.set_zoom(0.01);
        assert_eq!(zoom.current_factor(), 0.1);
    }
    
    #[test]
    fn test_zoom_locking() {
        let mut zoom = ZoomState::new(1.0);
        
        // Lock zoom
        zoom.lock();
        
        // Try to change - should fail
        let transition = zoom.set_zoom(5.0);
        assert_eq!(transition, ZoomTransition::None);
        assert_eq!(zoom.current_factor(), 1.0);
        
        // Unlock and try again
        zoom.unlock();
        let transition = zoom.set_zoom(5.0);
        assert_eq!(transition, ZoomTransition::StartTransition);
        assert_eq!(zoom.current_factor(), 5.0);
    }
}