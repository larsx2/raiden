use crate::{Area, panel_origin};

pub const FILE: &str = "window-size";

pub const OLD_FILE: &str = "expanded-views";

pub const EXPANDED: (f64, f64) = (900.0, 660.0);

const OLD_VIEWS: [&str; 4] = ["ai", "pulls", "clipboard", "settings"];

/// ```
/// use raiden_core::window::expanded_in;
/// assert!(expanded_in("expanded\n"));
/// assert!(!expanded_in("compact\n"));
/// assert!(!expanded_in(""));
/// ```
pub fn expanded_in(text: &str) -> bool {
    text.trim() == "expanded"
}

/// ```
/// use raiden_core::window::size_text;
/// assert_eq!(size_text(true), "expanded\n");
/// assert_eq!(size_text(false), "compact\n");
/// ```
pub fn size_text(expanded: bool) -> &'static str {
    if expanded { "expanded\n" } else { "compact\n" }
}

/// ```
/// use raiden_core::window::any_view_expanded;
/// assert!(any_view_expanded("pulls\n"));
/// assert!(any_view_expanded("search\n\n settings \n"));
/// assert!(!any_view_expanded("search\n\n"));
/// ```
pub fn any_view_expanded(old: &str) -> bool {
    old.lines().any(|line| OLD_VIEWS.contains(&line.trim()))
}

/// ```
/// use raiden_core::{Area, window::panel_frame};
/// let screen = Area { x: 0.0, y: 0.0, width: 1440.0, height: 900.0 };
/// let visible = Area { x: 0.0, y: 25.0, width: 1440.0, height: 875.0 };
/// let compact = panel_frame(screen, visible, (720.0, 452.0), (720.0, 452.0));
/// assert_eq!((compact.x, compact.y, compact.width, compact.height), (360.0, 198.0, 720.0, 452.0));
/// ```
pub fn panel_frame(screen: Area, visible: Area, compact: (f64, f64), wanted: (f64, f64)) -> Area {
    let (_, top) = panel_origin(screen, compact.0, compact.1);
    let width = wanted.0.min(visible.width);
    let height = wanted.1.min(visible.height);
    let centre = screen.x + screen.width / 2.0;
    let x = (centre - width / 2.0)
        .min(visible.x + visible.width - width)
        .max(visible.x);
    let y = top.min(visible.y + visible.height - height).max(visible.y);
    Area {
        x,
        y,
        width,
        height,
    }
}

pub fn cocoa_frame(frame: Area, primary_height: f64) -> Area {
    Area {
        x: frame.x.round(),
        y: (primary_height - frame.y - frame.height).round(),
        width: frame.width.round(),
        height: frame.height.round(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPACT: (f64, f64) = (720.0, 452.0);

    fn frame(area: Area) -> (f64, f64, f64, f64) {
        (area.x, area.y, area.width, area.height)
    }

    #[test]
    fn the_expanded_panel_keeps_its_top_and_centre_when_it_fits() {
        let screen = Area {
            x: 0.0,
            y: 0.0,
            width: 1728.0,
            height: 1117.0,
        };
        let visible = Area {
            x: 0.0,
            y: 33.0,
            width: 1728.0,
            height: 1084.0,
        };
        let compact = panel_frame(screen, visible, COMPACT, COMPACT);
        let expanded = panel_frame(screen, visible, COMPACT, EXPANDED);
        assert_eq!(compact.y, expanded.y);
        assert_eq!(compact.x + 360.0, expanded.x + 450.0);
        assert_eq!(frame(expanded), (414.0, 245.74, 900.0, 660.0));
    }

    #[test]
    fn the_expanded_panel_moves_up_just_enough_to_clear_the_dock() {
        let screen = Area {
            x: 0.0,
            y: 0.0,
            width: 1440.0,
            height: 900.0,
        };
        let visible = Area {
            x: 0.0,
            y: 25.0,
            width: 1440.0,
            height: 805.0,
        };
        assert_eq!(
            frame(panel_frame(screen, visible, COMPACT, EXPANDED)),
            (270.0, 170.0, 900.0, 660.0)
        );
        assert_eq!(
            frame(panel_frame(screen, visible, COMPACT, COMPACT)),
            (360.0, 198.0, 720.0, 452.0)
        );
    }

    #[test]
    fn the_expanded_panel_is_capped_to_a_small_screen() {
        let screen = Area {
            x: 0.0,
            y: 0.0,
            width: 1024.0,
            height: 640.0,
        };
        let visible = Area {
            x: 0.0,
            y: 25.0,
            width: 860.0,
            height: 555.0,
        };
        assert_eq!(
            frame(panel_frame(screen, visible, COMPACT, EXPANDED)),
            (0.0, 25.0, 860.0, 555.0)
        );
    }

    #[test]
    fn the_expanded_panel_stays_on_a_second_screen() {
        let screen = Area {
            x: 1440.0,
            y: -180.0,
            width: 1920.0,
            height: 900.0,
        };
        let visible = Area {
            x: 1440.0,
            y: -155.0,
            width: 1920.0,
            height: 805.0,
        };
        assert_eq!(
            frame(panel_frame(screen, visible, COMPACT, COMPACT)),
            (2040.0, 18.0, 720.0, 452.0)
        );
        assert_eq!(
            frame(panel_frame(screen, visible, COMPACT, EXPANDED)),
            (1950.0, -10.0, 900.0, 660.0)
        );
    }

    #[test]
    fn frames_flip_to_whole_points_from_the_bottom_of_the_main_screen() {
        let on_main = Area {
            x: 414.0,
            y: 245.74,
            width: 900.0,
            height: 660.0,
        };
        assert_eq!(
            frame(cocoa_frame(on_main, 1117.0)),
            (414.0, 211.0, 900.0, 660.0)
        );
        let above_main = Area {
            x: 1950.0,
            y: -10.0,
            width: 900.0,
            height: 660.0,
        };
        assert_eq!(
            frame(cocoa_frame(above_main, 900.0)),
            (1950.0, 250.0, 900.0, 660.0)
        );
    }

    #[test]
    fn a_dock_on_the_left_pushes_a_wide_panel_right() {
        let screen = Area {
            x: 0.0,
            y: 0.0,
            width: 1000.0,
            height: 900.0,
        };
        let visible = Area {
            x: 80.0,
            y: 25.0,
            width: 920.0,
            height: 875.0,
        };
        assert_eq!(
            frame(panel_frame(screen, visible, COMPACT, EXPANDED)),
            (80.0, 198.0, 900.0, 660.0)
        );
    }
}
