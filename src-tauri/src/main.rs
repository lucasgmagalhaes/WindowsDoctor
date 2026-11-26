// Evita abrir um console extra no Windows em builds de release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Chamado pelo Agendador de Tarefas: limpa sem abrir janela e sai.
    if std::env::args().any(|a| a == "--auto-cleanup") {
        std::process::exit(windows_folder_map_lib::run_auto_cleanup());
    }
    windows_folder_map_lib::run()
}
