use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EditorTool {
    #[default]
    Select,
    Move,
    Rotate,
    Scale,
}

impl EditorTool {
    pub const ALL: [Self; 4] = [Self::Select, Self::Move, Self::Rotate, Self::Scale];

    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Move => "Move",
            Self::Rotate => "Rotate",
            Self::Scale => "Scale",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AxisConstraint {
    #[default]
    None,
    X,
    Y,
    Z,
}

impl AxisConstraint {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolState {
    pub active: EditorTool,
    pub axis: AxisConstraint,
    pub show_gizmo: bool,
}

impl Default for ToolState {
    fn default() -> Self {
        Self {
            active: EditorTool::Select,
            axis: AxisConstraint::None,
            show_gizmo: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_state_defaults_to_select_with_gizmo() {
        let state = ToolState::default();
        assert_eq!(state.active, EditorTool::Select);
        assert_eq!(state.axis, AxisConstraint::None);
        assert!(state.show_gizmo);
    }

    #[test]
    fn tool_labels_are_stable() {
        assert_eq!(EditorTool::Move.label(), "Move");
        assert_eq!(EditorTool::Rotate.label(), "Rotate");
        assert_eq!(AxisConstraint::X.label(), "X");
        assert_eq!(AxisConstraint::None.label(), "None");
    }
}
