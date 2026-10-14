//! Keep the camera window inside usable desktop space, including on restore,
//! camera-driven resize, DPI transitions and manual moves.
use super::*;

fn bounded(mut rect: RECT, area: RECT) -> RECT {
    let width = (rect.right - rect.left).clamp(1, (area.right - area.left).max(1));
    let height = (rect.bottom - rect.top).clamp(1, (area.bottom - area.top).max(1));
    rect.left = rect
        .left
        .clamp(area.left, (area.right - width).max(area.left));
    rect.top = rect
        .top
        .clamp(area.top, (area.bottom - height).max(area.top));
    rect.right = rect.left + width;
    rect.bottom = rect.top + height;
    rect
}

fn avoid_bar(rect: RECT, area: RECT, bar: Option<RECT>) -> RECT {
    let rect = bounded(rect, area);
    let Some(bar) = bar else {
        return rect;
    };
    if rect.left >= bar.right
        || rect.right <= bar.left
        || rect.top >= bar.bottom
        || rect.bottom <= bar.top
    {
        return rect;
    }
    // Normally rcWork already excludes docked bars. This also covers a floating
    // bar and the interval while Explorer is still updating its reservation.
    let candidates = [
        RECT {
            top: bar.bottom.max(area.top),
            ..area
        },
        RECT {
            bottom: bar.top.min(area.bottom),
            ..area
        },
        RECT {
            left: bar.right.max(area.left),
            ..area
        },
        RECT {
            right: bar.left.min(area.right),
            ..area
        },
    ];
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    candidates
        .into_iter()
        .filter(|a| a.right > a.left && a.bottom > a.top)
        .min_by_key(|a| {
            let candidate = bounded(rect, *a);
            // Prefer retaining the camera window size, then the shortest move.
            (
                width - (candidate.right - candidate.left) + height
                    - (candidate.bottom - candidate.top),
                i64::from(candidate.left - rect.left).abs()
                    + i64::from(candidate.top - rect.top).abs(),
            )
        })
        .map_or(rect, |a| bounded(rect, a))
}

pub(super) unsafe fn constrain(hwnd: HWND, position: &mut WINDOWPOS) {
    if IsIconic(hwnd) != 0 {
        return;
    }
    let mut current: RECT = zeroed();
    if GetWindowRect(hwnd, &mut current) == 0 {
        return;
    }
    let x = if position.flags & SWP_NOMOVE != 0 {
        current.left
    } else {
        position.x
    };
    let y = if position.flags & SWP_NOMOVE != 0 {
        current.top
    } else {
        position.y
    };
    // Windows uses off-desktop coordinates during minimization; never turn that
    // transition into an unwanted restoration of the main window.
    if x <= -30000 || y <= -30000 {
        return;
    }
    let width = if position.flags & SWP_NOSIZE != 0 {
        current.right - current.left
    } else {
        position.cx
    };
    let height = if position.flags & SWP_NOSIZE != 0 {
        current.bottom - current.top
    } else {
        position.cy
    };
    let proposed = RECT {
        left: x,
        top: y,
        right: x.saturating_add(width),
        bottom: y.saturating_add(height),
    };
    let mut monitor = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..zeroed()
    };
    if GetMonitorInfoW(
        MonitorFromRect(&proposed, MONITOR_DEFAULTTONEAREST),
        &mut monitor,
    ) == 0
    {
        return;
    }
    let bar = BAR_WINDOW.with(|bar| {
        let mut rect: RECT = zeroed();
        (IsWindowVisible(bar.get()) != 0 && GetWindowRect(bar.get(), &mut rect) != 0)
            .then_some(rect)
    });
    let corrected = avoid_bar(proposed, monitor.rcWork, bar);
    if corrected.left != x || corrected.top != y {
        position.x = corrected.left;
        position.y = corrected.top;
        position.flags &= !SWP_NOMOVE;
    }
    if corrected.right - corrected.left != width || corrected.bottom - corrected.top != height {
        position.cx = corrected.right - corrected.left;
        position.cy = corrected.bottom - corrected.top;
        position.flags &= !SWP_NOSIZE;
    }
}

pub(super) unsafe fn ensure(hwnd: HWND) {
    if !hwnd.is_null() && IsWindow(hwnd) != 0 && IsIconic(hwnd) == 0 {
        let mut position = WINDOWPOS {
            hwnd,
            flags: SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
            ..zeroed()
        };
        constrain(hwnd, &mut position);
        if position.flags & (SWP_NOMOVE | SWP_NOSIZE) != SWP_NOMOVE | SWP_NOSIZE {
            SetWindowPos(
                hwnd,
                null_mut(),
                position.x,
                position.y,
                position.cx,
                position.cy,
                position.flags,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn title_bar_remains_below_topbar_at_different_dpis_and_monitor_origins() {
        for dpi in [96, 130, 144, 192] {
            let top = -1440 + scale(52, dpi);
            let area = RECT {
                left: -2560,
                top,
                right: 0,
                bottom: 0,
            };
            let result = bounded(
                RECT {
                    left: -2500,
                    top: -1440,
                    right: -2100,
                    bottom: -1000,
                },
                area,
            );
            assert_eq!(result.top, top);
            assert_eq!(result.right - result.left, 400);
            assert_eq!(result.bottom - result.top, 440);
        }
    }
    #[test]
    fn oversized_restores_and_floating_bars_leave_the_title_reachable() {
        let area = RECT {
            left: 0,
            top: 70,
            right: 1920,
            bottom: 1040,
        };
        let result = bounded(
            RECT {
                left: -300,
                top: -100,
                right: 2700,
                bottom: 1800,
            },
            area,
        );
        assert_eq!(
            [result.left, result.top, result.right, result.bottom],
            [0, 70, 1920, 1040]
        );
        let bar = RECT {
            left: 100,
            top: 100,
            right: 800,
            bottom: 152,
        };
        let result = avoid_bar(
            RECT {
                left: 150,
                top: 100,
                right: 550,
                bottom: 500,
            },
            area,
            Some(bar),
        );
        assert_eq!(result.top, 152);
        assert_eq!(result.bottom - result.top, 400);
    }
}
