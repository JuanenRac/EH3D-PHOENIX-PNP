# EH3D-PHOENIX-PNP: scope

Status: **simulator**. EH3D-PHOENIX-PNP is a library, a command line and a window for the assembly of boards by pick and place. Everything it does with a machine is a simulation: a simulated machine, a simulated camera and a simulated G-code controller. It has not placed a part, no port has been opened by it and no real camera has been read. This page says what it is for, what it will not do and where it sits among the other programs of the suite.

## Purpose

The program of the PHOENIX family that takes the position files of a board, gives every part the feeder that holds it and plans the order in which the parts are placed, for a head with one to six nozzles; it carries the plan out on a simulated machine and writes what a person or another program needs to do the same work: a plan, a load sheet, a report and the files that an assembly house asks for. It also walks a person through a board that is placed by hand.

It is for the assembly of boards and of their components. It is not a program for taking up objects, for arms or for robots.

## What it does

* Reads the position files of KiCad, Altium, EAGLE and assembly houses, and tables of parts, feeders, packages and bills of materials, with the unit, the side and the turn of each part made explicit.
* Assigns the feeder of each part (tape, tray, strip, tube, bulk), plans the order of the placements (ranks, nozzle tips, angle limits, the time of the moves, panels with fiducials and boards marked as bad) and says what is missing before anything is done.
* Carries a job out on a simulated machine with a simulated camera, vacuum and controller, with retries, faults that can be forced and a report; the job can be paused, stepped, resumed and saved.
* Describes a G-code driver (Marlin and Klipper) as a profile file, tries it against a simulated controller and applies rules that only make the machine safer, among them the interlocks of the axes.
* Reads the files of OpenPnP without ever writing to them.
* Walks a person through a board phase by phase (loading, placing by hand, the oven, soldering by hand, looking) in the guided manual mode.
* Shows all of this in a window made of the panels of the docking frame of the suite, in eight languages, and as a command line whose commands can also be run from panels of the window.
* Talks to the EH3D server through the common client of the suite (settings, feeders, calibrations and a dry run), tried against a server in memory.

## What it will not do

* No handling of objects, no arms and no robots: that is another family of programs.
* No decision about the limits of a machine, its interlocks or its emergency stop (see below).
* No writing of files for OpenPnP: the import is read only.
* No live camera: the vision program of the suite owns the live capture; here the camera is a model and a made-up picture.
* No paste dispensing, no nozzle changer sequence and no jog yet (see `NOT_YET.md`).
* No claim that anything was proven on a machine: the evidence of every capability is `simulator` or `host-test`.

## Where it sits

* **It reads:** position files and tables on disk, its own settings file (`machine.cfg`), driver profiles, recipes and process files; and, when asked, the files of OpenPnP.
* **It writes:** a plan (JSON), a load sheet, a report (text, HTML, CSV and JSON), the files for assembly houses and the state of a job; never over a file it was made from.
* **It is read by:** the pick and place task of the EH3D server and the other programs of the suite through those files; programs exchange work through files or through the server, never by depending on each other.
* **It depends on:** `EH3D-COMMON` for the client of the server, the panel of the settings of the server and the docking frame of the windows, built next to it as a sibling folder at version 0.3.0.

## What it never decides

For a program that drives a machine, the limits, the interlocks and the emergency stop belong to the machine and to its controller, not to this program. The rules here (the limits of the profile, the interlocks, the levels of safety) can only make a move refused or slower and never faster or wider; a refused move leaves the machine in the state of `safe_stop` and stops the session; a signal that nobody has reported is unknown and does not satisfy a rule. Nothing here replaces the emergency stop of a machine or the judgement of the person who stands at it.

The requirements of the program, with the state of each and the test that shows it, are in `REQUIREMENTS.md`; the state in a paragraph is in `STATUS.md`.
