# The window on Windows: why it is a program of its own

On Windows a program is built for one of two subsystems. A program of the **console** subsystem always has a console window: started from a terminal it prints in that terminal, started with a double click it opens a black console window of its own behind its window. A program of the **graphical** subsystem opens no console, and so it cannot print in the terminal that started it.

`phoenix-pnp` is a command line: its output is the result (`parts`, `plan`, `check-bottom`, ...) and its exit code is read by scripts and by the CI, so it must keep its console. The window must not open a black console behind it. Three ways of getting both were looked at.

## 1. `windows_subsystem = "windows"` on `phoenix-pnp` when the `gui` feature is on

`#![cfg_attr(all(windows, feature = "gui"), windows_subsystem = "windows")]` is the smallest change, but it takes the console away from the command line in every build that has the window: a terminal shows nothing of what `phoenix-pnp plan ...` prints, and the person must remember to build without the feature to use the command line. A build with the feature is the build everybody uses, so the command line would be unusable in practice. Not chosen.

## 2. Hiding the console while the program runs

Closing or hiding the console after the program has started (`FreeConsole`, `ShowWindow`) needs a call into the system library, which is `unsafe`. The program and the library are `#![forbid(unsafe_code)]`, and that rule is worth more than a window that does not flash. Not chosen.

## 3. A second program for the window (chosen)

`phoenix-pnp-gui` (`src/gui_main.rs`) is built only with `--features gui` and is a program of the graphical subsystem (`cfg_attr(windows, windows_subsystem = "windows")`; on other systems the attribute does nothing). It opens the window with the files named on its command line, and that is all it does.

* `phoenix-pnp` stays a console program in every build. Its output and its exit codes work in a terminal, in a script and in the CI. `phoenix-pnp` with no command and `phoenix-pnp gui FILES` still open the window, from a terminal that already has a console.
* A shortcut, the start menu or an association of the files with the program should point at `phoenix-pnp-gui`: no console opens.
* A program without a console has nowhere to print. An error that keeps the window from opening is written to `last_error.txt`, beside the settings of the window (`%APPDATA%\EH3D-PHOENIX-PNP\` on Windows), and the exit code is 1.
* `cargo build --release --features gui` builds both programs and both carry the same icon and version information (`build.rs`); without the feature only `phoenix-pnp` is built.

What has not been checked: that the second program opens without a console on a real Windows desktop. The CI compiles it with the lints of the window on Windows and on Linux, but nobody has looked at it on a screen yet.
