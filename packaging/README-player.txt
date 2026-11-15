Sega Racing Arcade Series Launcher
==================================

One launcher for six Sega arcade racers: OutRun 2 SP SDX, OutRun 2 SP
Special Tours, Crazy Taxi High Roller, SEGA Rally 3, SEGA Racing Classic and
Daytona Championship USA. It contains no game files: you need your own
TeknoParrot dumps, one folder each.

Start
-----

Linux: run ./sega-launcher from this folder.
Windows: run sega-launcher.exe.

The first window asks for the folder that holds your game folders. Add it,
and the launcher lists what it found. For the two Chihiro games, add your
Chihiro BIOS folder under Settings, Folders and BIOS.

What works today
----------------

- SEGA Rally 3 and SEGA Racing Classic: full races (Linux).
- OutRun 2 SP SDX: full races (Linux only). Its loader
  is built on your computer the first time you press Play: that needs git
  and docker or podman, and takes a few minutes.
- Daytona Championship USA: full races (Linux).
- OutRun 2 SP Special Tours, Crazy Taxi High Roller: not yet; the window
  says why.

Controls
--------

A wheel or a pad. Pad: triggers are the pedals, the left stick steers,
the shoulder buttons shift, Y changes the view, B is the handbrake, Start
starts, Back inserts a coin. Each game's steering lock and force feedback
strength are in Settings.

Where things live
-----------------

Linux: ~/.local/share/rea-sega/ (settings, each game's runtime with its
rankings and log, the Wine prefix). Windows: %LOCALAPPDATA%\rea-sega\.
To uninstall, delete this folder and that one (copy runtimes/ first to keep
your rankings).

More: https://github.com/akitaonrails/rea-teknoparrot-sega-series
Licences and sources: the licenses/ folder.
