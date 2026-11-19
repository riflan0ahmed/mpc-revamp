//! Pan & scan steps (MPC's sizes and moves).

use mpc_rv::view::PanScan;

#[test]
fn pan_scan_steps() {
    let mut p = PanScan::default();
    p.step(1, 1, 0, 0);
    assert!((p.zoom_x - 1.02).abs() < 1e-9 && (p.zoom_y - 1.02).abs() < 1e-9);
    for _ in 0..500 {
        p.step(1, 0, 1, 0);
    }
    assert!(p.zoom_x >= 3.0 && p.zoom_x < 3.0 * 1.02 && p.pos_x == 1.0); // stops growing once past 3
    p.step(0, -1, -1, -1);
    assert!(p.zoom_y < 1.02 && p.pos_x < 1.0 && p.pos_y < 0.5);
}
