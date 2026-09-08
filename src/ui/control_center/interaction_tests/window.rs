use super::*;

#[test]
fn restored_window_rect_is_fully_inside_work_area() {
    let work = RECT {
        left: 0,
        top: 0,
        right: 1000,
        bottom: 800,
    };
    let saved_rects = [
        RECT {
            left: 100,
            top: -500,
            right: 700,
            bottom: 100,
        },
        RECT {
            left: 100,
            top: 750,
            right: 700,
            bottom: 1350,
        },
        RECT {
            left: -1500,
            top: 100,
            right: -900,
            bottom: 700,
        },
        RECT {
            left: 1500,
            top: 100,
            right: 2100,
            bottom: 700,
        },
        RECT {
            left: -5000,
            top: -5000,
            right: 5000,
            bottom: 5000,
        },
    ];
    for saved in saved_rects {
        let clamped = clamp_window_rect(saved, work);
        assert!(clamped.left >= work.left);
        assert!(clamped.top >= work.top);
        assert!(clamped.right <= work.right);
        assert!(clamped.bottom <= work.bottom);
    }

    let negative_work = RECT {
        left: -1920,
        top: -100,
        right: 0,
        bottom: 980,
    };
    let clamped = clamp_window_rect(
        RECT {
            left: -5000,
            top: -5000,
            right: -4000,
            bottom: -4000,
        },
        negative_work,
    );
    assert!(clamped.left >= negative_work.left);
    assert!(clamped.top >= negative_work.top);
    assert!(clamped.right <= negative_work.right);
    assert!(clamped.bottom <= negative_work.bottom);
}

#[test]
fn fixed_window_size_is_dpi_scaled_and_not_user_sized() {
    assert_eq!(fixed_window_size(96), (960, 660));
    assert_eq!(fixed_window_size(144), (1440, 990));
    assert_eq!(fixed_window_size(192), (1920, 1320));
    assert_eq!(fixed_window_size(0), (960, 660));

    let work = RECT {
        left: 0,
        top: 0,
        right: 2400,
        bottom: 1600,
    };
    let (width, height) = fixed_window_size(144);
    let rect = clamp_window_rect(
        RECT {
            left: 100,
            top: 100,
            right: 100 + width,
            bottom: 100 + height,
        },
        work,
    );
    assert_eq!(rect.right - rect.left, width);
    assert_eq!(rect.bottom - rect.top, height);
}
