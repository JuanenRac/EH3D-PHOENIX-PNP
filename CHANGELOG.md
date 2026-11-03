# Changelog

All notable changes to this project are documented here.

## [Unreleased]
- **Safer files:** a plan is written through a temporary file, text that is not UTF-8 is read as Latin-1 with a note, and a fractional `--nozzles` is refused.
- **One side at a time:** a plan for one side no longer asks feeders for the parts of the other side nor uses them up, and a position column whose header names its unit (`PosX(mil)`, `Center-X(in)`) reads bare numbers in that unit.
- **The workflows are hardened:** every action is pinned by its commit with its version in a comment, and the checkouts keep no credentials.
- **More tests:** the units and the delimiters of the position files, the rows that are wrong, the limit of parts, the KiCad position file and the file of feeders.

## [0.0.3] - CI hardening and shared validator

- **Faster, bounded CI:** the Cargo build is cached, a run that a newer push supersedes is cancelled, every job has a time limit, and the Rust jobs live in their own workflow that is skipped when only documents or images change.
- **A fixed Rust toolchain:** `rust-toolchain.toml` names the compiler version (with rustfmt and clippy) that the CI installs, so a new stable release cannot turn the lints red without a change here.
- **Monthly dependency updates:** Dependabot opens one grouped pull request a month for the GitHub Actions and the Cargo dependencies.
- **A line for every change:** a pull request that changes code must add a line under `[Unreleased]` of this file (`tools/changelog_pr_check.py`; the label `no-changelog` skips it).
- **Shared checks updated:** the validator of the suite checks the text of the banner against the manifest and reads the files that git tracks.

## [0.0.2] - More tests and a KiCad file with a byte order mark

- **Strict command line:** an option that is written wrong (`--powr`) or a word too many is an error that names it; before, it was ignored and the output was made with the default values.
- **The banner of the README** shows the role and the technologies of this program (it showed the text of the shared library).
- **More tests:** the position files are checked with a byte order mark, tabs, units and angles of any size; the feeders with exhausted tapes and non-ASCII names; the plan with exact travel and time, mirroring twice and the same result on two runs; and every command of the command line. A KiCad position file saved with a byte order mark is now read.

## [0.0.1] - Pick and place assembly: the position files, the feeders and the order of the placements

- **The parts:** KiCad `.pos` and CSV position files in mm, mil or inches, with the angles normalised and a damaged row refused with its number.
- **The feeders:** a CSV of feeders; every part gets the feeder of its value and package; parts without a feeder and feeders that run out are reported.
- **The plan:** nearest-first placements cut in batches of one to six nozzles, the bottom side mirrored, the travel and an estimate of the time, as a JSON job.
- **The command line** `eh3d-pnp` with `parts`, `check` and `plan`, and a sample board with its feeders.
- **14 tests** run on a computer, with damaged input.
