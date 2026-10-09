//! macOS main-thread entry: application setup on the process's real main thread.
//!
//! libtest runs every `#[test]` on a worker thread, so no `#[test]` can show what happens when an
//! application is created on the OS main thread. With no argument this program runs each check
//! below in its own child process (fresh application singleton and thread-locals), on that child's
//! main thread, and kills a child that runs past `CHILD_TIMEOUT`. It exits 1 if any check failed
//! or timed out.
//!
//! - `gui`: `GuiApplication::new` must create `CocoaEventDispatcher`, and its event loop must
//!   deliver a qtrs timer event and return the code that event passed to `exit`.
//! - `core`: the same with `CoreApplication::new`, which must create `UnixEventDispatcher`.
//!
//! Qt: `QCoreApplication` gets `QEventDispatcherUNIX` on Darwin (qcoreapplication.cpp:518-523,
//! qthread_unix.cpp:316-325); `QGuiApplication` takes its dispatcher from the platform plugin
//! (qguiapplication.cpp:1629-1643), `QCocoaEventDispatcher` on macOS (qcocoaintegration.mm:353-356).
//!
//! Only qtrs events are covered. The Cocoa dispatcher does not dispatch `NSEvent`s or run the
//! CFRunLoop yet (Contract G7.6.h), so a pass says nothing about native event delivery.
//!
//! Usage: cargo run -p qtrs-gui --example main_thread_entry   (macOS only)

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("main_thread_entry checks macOS behavior only");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = match args.get(1).map(String::as_str) {
        None => macos::supervise(&args[0]),
        Some(mode) => macos::run_child(mode),
    };
    std::process::exit(code);
}

#[cfg(target_os = "macos")]
mod macos {
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use qtrs_core::application::CoreApplication;
    use qtrs_core::event_loop::{CocoaEventDispatcher, UnixEventDispatcher};
    use qtrs_core::timer::Timer;
    use qtrs_gui::application::GuiApplication;

    const MODES: [&str; 2] = ["gui", "core"];
    const CHILD_TIMEOUT: Duration = Duration::from_secs(30);
    const EXIT_CODE: i32 = 7;

    extern "C" {
        fn pthread_main_np() -> std::os::raw::c_int;
    }

    fn on_os_main_thread() -> bool {
        unsafe { pthread_main_np() == 1 }
    }

    pub fn supervise(exe: &str) -> i32 {
        let mut failed = 0;
        for mode in MODES {
            let start = Instant::now();
            let mut child = Command::new(exe).arg(mode).spawn().expect("spawn child");
            let status = loop {
                if let Some(status) = child.try_wait().expect("wait for child") {
                    break Some(status);
                }
                if start.elapsed() > CHILD_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(20));
            };
            let ok = status.is_some_and(|s| s.success());
            let how = match status {
                Some(s) => s.to_string(),
                None => format!("TIMEOUT after {} s", CHILD_TIMEOUT.as_secs()),
            };
            let verdict = if ok { "ok" } else { "FAILED" };
            let secs = start.elapsed().as_secs_f64();
            println!("{verdict:6} {mode} ({how}, {secs:.1} s)");
            failed += usize::from(!ok);
        }
        println!("{} checks, {failed} not ok", MODES.len());
        i32::from(failed > 0)
    }

    /// Posts a qtrs timer event that records it ran and asks the loop to exit with `EXIT_CODE`.
    fn schedule_exit(exit: fn(i32)) -> Arc<AtomicBool> {
        let fired = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&fired);
        Timer::single_shot(20, move || {
            flag.store(true, Ordering::SeqCst);
            exit(EXIT_CODE);
        });
        fired
    }

    pub fn run_child(mode: &str) -> i32 {
        // The thread check must itself tell threads apart, or the rest proves nothing.
        let main = on_os_main_thread();
        let worker = std::thread::spawn(on_os_main_thread).join().unwrap();
        println!("[{mode}] pthread_main_np: first thread {main}, spawned thread {worker}");
        if !main || worker {
            println!("[{mode}] FAIL: not on the OS main thread, or the check cannot tell");
            return 1;
        }

        let args = vec!["main_thread_entry".to_string()];
        let (expected, dispatcher, code, fired) = match mode {
            "gui" => {
                let mut app = GuiApplication::new(args);
                let dispatcher = CoreApplication::event_dispatcher_class_name();
                let fired = schedule_exit(GuiApplication::exit);
                let code = app.exec();
                let expected = std::any::type_name::<CocoaEventDispatcher>();
                (expected, dispatcher, code, fired)
            }
            "core" => {
                let mut app = CoreApplication::new(args);
                let dispatcher = CoreApplication::event_dispatcher_class_name();
                let fired = schedule_exit(CoreApplication::exit);
                let code = app.exec();
                let expected = std::any::type_name::<UnixEventDispatcher>();
                (expected, dispatcher, code, fired)
            }
            other => {
                println!("unknown mode `{other}`; expected one of {MODES:?}");
                return 2;
            }
        };
        let fired = fired.load(Ordering::SeqCst);
        println!(
            "[{mode}] dispatcher {dispatcher:?}, exec returned {code}, timer event ran {fired}"
        );

        let mut ok = true;
        if dispatcher != Some(expected) {
            println!("[{mode}] FAIL: expected dispatcher {expected}");
            ok = false;
        }
        if !fired || code != EXIT_CODE {
            println!(
                "[{mode}] FAIL: expected the timer event to run and exec to return {EXIT_CODE}"
            );
            ok = false;
        }
        i32::from(!ok)
    }
}
