//! Standalone Snake executable launcher.
//! Compiles and runs the Nokia 3310 Snake game purely written in HWCode (`snake.hwc`).

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let target = if args.len() >= 2 {
        &args[1]
    } else {
        "snake.hwc"
    };
    hwcode::runner::run_game(target);
}
