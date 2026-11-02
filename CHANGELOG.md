# Changelog

All notable changes to this project are documented here.

## [Unreleased]

- **Strict command line:** an option that is written wrong (`--powr`) or a word too many is an error that names it; before, it was ignored and the output was made with the default values.
- **The banner of the README** shows the role and the technologies of this program (it showed the text of the shared library).

## [0.0.1] - Pick and place assembly: the position files, the feeders and the order of the placements

- **The parts:** KiCad `.pos` and CSV position files in mm, mil or inches, with the angles normalised and a damaged row refused with its number.
- **The feeders:** a CSV of feeders; every part gets the feeder of its value and package; parts without a feeder and feeders that run out are reported.
- **The plan:** nearest-first placements cut in batches of one to six nozzles, the bottom side mirrored, the travel and an estimate of the time, as a JSON job.
- **The command line** `eh3d-pnp` with `parts`, `check` and `plan`, and a sample board with its feeders.
- **14 tests** run on a computer, with damaged input.
