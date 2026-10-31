# Changelog

All notable changes to this project are documented here.

## [Unreleased]

## [0.0.1] - Pick and place assembly: the position files, the feeders and the order of the placements

- **The parts:** KiCad `.pos` and CSV position files in mm, mil or inches, with the angles normalised and a damaged row refused with its number.
- **The feeders:** a CSV of feeders; every part gets the feeder of its value and package; parts without a feeder and feeders that run out are reported.
- **The plan:** nearest-first placements cut in batches of one to six nozzles, the bottom side mirrored, the travel and an estimate of the time, as a JSON job.
- **The command line** `eh3d-pnp` with `parts`, `check` and `plan`, and a sample board with its feeders.
- **14 tests** run on a computer, with damaged input.
