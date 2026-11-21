// Evita abrir um console extra no Windows em builds de release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    windows_folder_map_lib::run()
}
