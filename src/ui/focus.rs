//! Semantic focus ownership shared by UI input handling and accessibility adapters.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FocusOwner {
    Settings,
    Picker,
    #[default]
    Outside,
}

/// Tab traversal is defined even for an empty or entirely disabled focus order.
pub(crate) fn next_focus_target(
    order: &[crate::ui::layout::ElementId],
    current: Option<crate::ui::layout::ElementId>,
    reverse: bool,
    disabled: impl Fn(crate::ui::layout::ElementId) -> bool,
) -> Option<crate::ui::layout::ElementId> {
    let last = order.len().checked_sub(1)?;
    let start = current
        .and_then(|id| order.iter().position(|candidate| *candidate == id))
        .unwrap_or(if reverse { 0 } else { last });
    let mut index = start;
    for _ in order {
        index = if reverse {
            index.checked_sub(1).unwrap_or(last)
        } else {
            (index + 1) % order.len()
        };
        let candidate = order[index];
        if !disabled(candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::next_focus_target;
    use crate::ui::layout::ElementId;

    #[test]
    fn empty_and_disabled_orders_have_no_target() {
        assert_eq!(next_focus_target(&[], None, false, |_| false), None);
        assert_eq!(
            next_focus_target(&[ElementId::Search], None, true, |_| true),
            None
        );
    }

    #[test]
    fn absent_focus_enters_at_the_requested_end() {
        let order = [ElementId::Search, ElementId::WindowClose];
        assert_eq!(
            next_focus_target(&order, None, false, |_| false),
            Some(order[0])
        );
        assert_eq!(
            next_focus_target(&order, None, true, |_| false),
            Some(order[1])
        );
    }
}
