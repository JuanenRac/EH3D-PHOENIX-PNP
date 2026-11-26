# The `.eh3d` project and the branch `pnp/`

An `.eh3d` file is the project of the EH3D Fabrication Suite: one ZIP file with a branch for each program (`pcb/`, `cad/`, `3dp/`, `laser/`, `cnc/`, `pnp/`...). The format is defined in `docs/FORMAT_EH3D.md` of EH3D-COMMON and read and written by its crate `eh3d-project`, whose module `appbranch` holds what every program does with its branch. `phoenix-pnp` writes only `pnp/`; every other branch, known or not, keeps its bytes. Nothing in a project is run and these commands move no machine.

## The commands

| Command | What it does |
|---|---|
| `project list FILE.eh3d [--json]` | the branches, who wrote each one, their format, which are signed, and what is out of date or wrong |
| `project save FILE.eh3d [--parts F] [--bom F] [--feeders F] [--config F] [--name N] [--id ID]` | reads each file with the reader of the program and puts it in `pnp/`; the project is created when it is not there (the name and the identifier are only for a new one); what is not given stays |
| `project import FILE.eh3d SOURCE --kind parts\|bom\|feeders [--format auto\|csv\|eagle\|kicad-pos] [--map MAP] [--config F]` | takes a position file, a bill of materials or a table of feeders, keeps it as it is under `pnp/origin/KIND/` with its report; a position file becomes a table of parts of the program (`--format` and `--map` are only for it) |
| `project take FILE.eh3d BRANCH NAME [--kind K] [--config F]` | copies a file of another branch, such as `pcb/`, to `pnp/taken/BRANCH/NAME` with a reference to the state of its source; positions, a bill of materials or feeders become the ones of the work, any other file is only kept |
| `project open FILE.eh3d [--into DIR] [--config F]` | reads and checks `pnp/` (with the configuration kept in it, else the one given) and says what it holds; with `--into` writes its files |
| `project extract FILE.eh3d BRANCH DIR` | writes the files of any branch, byte for byte |

`save` refuses a settings file of `--config` that gives a value to `server_token` and writes nothing: a project is passed on and the token must not travel in it. Take the line out or save without `--config`. A project saved before with such a file should be looked at and the token changed.

The exit code is 0 when all is well, 2 when something is out of date or wrong (`list` and `open`), and 1 when the command could not be run. A file that is there is never written over by `open` or `extract`. The project is replaced in one step (a temporary file, then a rename).

Writing the branch removes a signature of the project, and a signature of `pnp/` when there is one; the command says so. These commands neither sign nor check signatures: `eh3d-project` of EH3D-COMMON does.

## What is in the branch

```
pnp/
  branch.json               written by the container: program, format 1.0, content hash, references
  pnp.json                  which file is the table of parts, the BOM, the feeders, the configuration
  parts/NAME                a table of parts given to `save`, or made from an imported position file
  bom/NAME
  feeders/NAME
  settings/machine.cfg
  origin/KIND/NAME          the original of the last import of that kind (parts, bom, feeders)
  origin/KIND/report.json   what was read and what is not kept in the table
  origin/report.json        the report of the last import of any kind
  taken/BRANCH/NAME         copies of files of other branches
```

Fields of `pnp.json` and of the record that this version does not know, and files it does not know, are kept. A branch of another format than 1.0 is not written (`EH3D-PKG-024`); a sealed branch or project is not written (`EH3D-PKG-020`).

## Out of date

A copy made with `take` keeps the content hash and the format of the branch it came from. When that branch changes, `list` and `open` say `OUT OF DATE`; take the file again to bring it up to date. Nothing is ever changed on its own.

## Limits

The program keeps one import of each kind: a second import of the same kind replaces the first and says so. Only text tables are read from other branches (`.csv`, `.pos`, `.txt`, `.tsv`); a board file is kept as a reference and is not read for positions. The files of OpenPnP are imported with `import-openpnp`, not into a project. It has only been tried with made-up projects.
