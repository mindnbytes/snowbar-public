//! Print Rune's startup display snapshot without creating any windows or overlays.
fn main() {
    rune::run_windows(|displays| {
        println!("Rune startup displays: {}", displays.len());
        for display in displays {
            println!(
                "name={:?} native_id={:?} position={:?} size={:?} scale={} primary={}",
                display.name,
                display.native_id(),
                display.position,
                display.size,
                display.scale,
                display.primary
            );
        }
        Vec::<rune::AnyWindowSpec>::new()
    });
}
