# Windows Folder Map

Desktop app (Tauri v2 with **Rust** + **React**) for looking after a Windows PC. It has four modules,
picked from the bar on the left:

| Module | What it does |
| --- | --- |
| **Disk map** | Shows **where disk space is being used** in each partition, from the drive root down to the last folder, so you can find what to delete. |
| **Cleanup** | Measures and empties temp folders and caches that Windows and apps rebuild on their own. |
| **Diagnostics** | Reads the event logs and the hardware to explain black screens, crashes, slowness and disk errors. Read only. |
| **Optimizations** | Reversible Windows tweaks (display, power, games, storage) and the list of programs that start with Windows. |

## Why sunburst/treemap instead of a graph

A node-and-edge graph turns unreadable at thousands of folders, and in it size is not
visual. Here area is proportional to size:

- **Sunburst**: each ring is one folder level. Large slices stand out right away and you can see the
  whole path from the drive to the deep folder at once.
- **Treemap**: the same data as nested rectangles. Good for spotting big individual files.
- **List** beside the chart, sorted largest first, with % of the parent folder and actions.
- **Largest files** tab: the 500 largest files on the scanned partition.

## Disk map

- Lists the partitions with used/free space.
- Parallel scan in Rust (rayon), with live progress and cancel.
- Navigate by clicking: click a folder to go in, click the centre / breadcrumb / `Backspace` to go up.
- **Cleanup hints** for known space hogs: `Windows.old`, `$Recycle.Bin`,
  `hiberfil.sys`, `Temp`, Windows Update cache, `node_modules`, package caches (npm, gradle, nuget…),
  WSL/Docker disks, `WinSxS` (with the right DISM command) and others.
- **Delete** to the Recycle Bin or permanently, with confirmation. The tree and totals update with no new scan.
- **Rescan** a single folder (⟳) after cleaning it by other means.
- Open any item in Explorer.
- Protections: the backend refuses to delete drive roots and anything inside `%SystemRoot%` (`C:\Windows`).
- Symlinks/junctions are ignored (no double counting or loops). OneDrive "online-only"
  files count as 0 bytes, since they do not take up local space.
- Files smaller than a configurable threshold (1 MB by default) are grouped per folder, which keeps
  memory low on partitions with millions of files.

## Cleanup

Only the *contents* of a fixed list of folders is deleted; the UI picks targets by id, never by path.
Files in use are skipped, and links/junctions are never followed.

- User temp (`%TEMP%`) and `Windows\Temp`: only files untouched for 24 hours.
- Windows Update download cache and Delivery Optimization cache (admin).
- GPU shader caches (DirectX, NVIDIA, AMD, Intel).
- Browser caches (Chrome, Edge, Brave, Firefox). Passwords, history and cookies are not touched.
- Windows Error Reporting and app crash dumps (BSOD minidumps are kept).
- Recycle Bin.

## Diagnostics

Runs a read-only PowerShell script (`crates/systools/src/diagnostics.ps1`) and evaluates the result in Rust.
Each check gets a status (OK / tip / warning / problem), what to do next and, when there is one, a button
that applies the matching optimization.

- **Black screen / display driver**: driver resets (`Display` 4101, `nvlddmkm` 13/14/153, AMD). A reset is
  the classic "screen goes black for ~2 seconds and comes back".
- Display driver age, or no driver at all (Microsoft Basic Display Adapter).
- Multiplane Overlay (MPO) enabled on NVIDIA, a known cause of black flicker.
- WHEA hardware errors, unexpected shutdowns (Kernel-Power 41), disk/NTFS errors and physical disk health.
- Free space on the system drive, memory use, uptime (and Fast Startup), apps that crashed in the last week.
- System summary: Windows, CPU, RAM, GPUs with driver and current mode, monitors with connection type, disks.
- **Copy report** puts a plain text summary on the clipboard, handy for support forums.

## Optimizations

Every tweak reads the real current state, so the switch is always right even if it was changed elsewhere,
and every one can be undone from the same switch.

- Display: disable MPO.
- System: disable Fast Startup.
- Power: High Performance plan, PCI Express link power management off, USB selective suspend off.
- Games: Game Mode, background recording (Game DVR).
- Storage: disable hibernation (deletes `hiberfil.sys`).
- **Startup programs** (Run keys and Startup folders), turned on/off the same way as Task Manager does
  (`StartupApproved`), so nothing is deleted.

Changes under `HKLM` need administrator rights. The app shows whether it is elevated and has a
**Reopen as administrator** button.

## Structure

```
crates/diskscan/     Scanner and tree (pure Rust, no Tauri; has tests)
crates/systools/     Cleanup, diagnostics, optimizations, startup programs (Windows APIs behind cfg; tests run anywhere)
src-tauri/           Tauri app: commands (scan, query, delete, Explorer, tools)
src/                 React frontend (d3-hierarchy for the layouts)
src/modules/         One component per module (DiskMap, Cleanup, Diagnostics, Optimize)
src/mockApi.ts       Simulated backend: lets you develop the UI in the browser
```

## Running

Prerequisites on Windows: [Rust](https://rustup.rs), Node 20+, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) (Visual Studio Build Tools with
"Desktop development with C++"; WebView2 already ships with Windows 10/11).

```powershell
npm install
npm run tauri dev      # development
npm run tauri build    # installer in target/release/bundle/
```

To include protected folders (other users, `System Volume Information`…), run the app
**as administrator**.

UI only, in the browser with fake data: `npm run dev` and open http://localhost:1420.

Scanner from the command line (handy for measuring performance):

```powershell
cargo run --release -p diskscan --example scan -- C:\
```

Tests: `cargo test -p diskscan -p systools`.

GitHub Actions (`.github/workflows/build.yml`) builds the Windows installer on every push.
Download it from the run's artifacts.

## Notes

- Sizes are **logical** (the file size). Compressed NTFS files can take up less on disk.
- Hard links (common in `C:\Windows\WinSxS`) are counted more than once, which is why WinSxS looks bigger
  than it really is.
- "Not accounted for" in the top bar is the space used on the partition that did not show up in the scan
  (NTFS metadata, shadow copies/restore points, folders without permission).
- Sending to the Recycle Bin **does not free space** until it is emptied.
