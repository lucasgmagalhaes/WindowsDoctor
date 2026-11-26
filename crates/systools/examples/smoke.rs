//! Roda as partes somente leitura do `systools` no Windows de verdade e falha
//! se algo vier vazio ou quebrado. Usado no CI (runner windows-latest):
//!
//! ```text
//! cargo run -p systools --example smoke
//! ```

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut failures = Vec::new();

    match systools::diagnostics::collect() {
        Ok(raw) => {
            println!("SO: {} | CPU: {} | RAM: {} bytes", raw.os, raw.cpu, raw.ram_total);
            println!(
                "GPUs: {} | monitores: {} | eventos: {} | discos: {}",
                raw.gpus.len(),
                raw.monitors.len(),
                raw.events.len(),
                raw.disks.len()
            );
            println!(
                "dispositivos com erro: {} | minidumps: {} | sensores: {} | reinício pendente: {} | bateria: {}",
                raw.devices.len(),
                raw.minidumps.len(),
                raw.temps.len(),
                raw.pending_reboot,
                raw.battery.is_some()
            );
            for d in &raw.disks {
                println!("disco {}: {} | SMART temp {:?} desgaste {:?}", d.name, d.health, d.temperature, d.wear);
            }
            if raw.os.trim().is_empty() {
                failures.push("diagnóstico: nome do Windows vazio".to_string());
            }
            if raw.cpu.trim().is_empty() {
                failures.push("diagnóstico: processador vazio".into());
            }
            if raw.ram_total == 0 {
                failures.push("diagnóstico: memória total zero".into());
            }
            if raw.system_total == 0 {
                failures.push("diagnóstico: tamanho da unidade do sistema zero".into());
            }
            let report = systools::diagnostics::evaluate(&raw);
            for c in &report.checks {
                println!("[{:?}] {}: {}", c.status, c.title, c.summary);
            }
        }
        Err(e) => failures.push(format!("diagnóstico: {e}")),
    }

    for m in systools::ddc::read_all() {
        println!("monitor DDC/CI: {} | modelo {:?} | firmware {:?} | ddc {}", m.description, m.model, m.firmware, m.ddc);
    }

    // ICMP costuma ser bloqueado nos runners: só confere que o script roda.
    match systools::network::run() {
        Ok(r) => {
            for c in &r.checks {
                println!("[rede {:?}] {}: {}", c.status, c.title, c.summary);
            }
        }
        Err(e) => failures.push(format!("rede: {e}")),
    }

    for t in systools::tweaks::list() {
        println!("ajuste {}: {:?} {}", t.id, t.applied, t.error.unwrap_or_default());
    }

    match systools::monitor::latest_record_id() {
        Ok(id) => println!("último evento de vídeo no log: {id}"),
        Err(e) => failures.push(format!("monitor: {e}")),
    }

    match systools::shadows::info() {
        Ok(i) => println!("pontos de restauração: {} cópias, {} bytes", i.copies, i.used),
        Err(e) => failures.push(format!("pontos de restauração: {e}")),
    }
    match systools::programs::list() {
        Ok(p) => println!("programas instalados: {}", p.len()),
        Err(e) => failures.push(format!("programas: {e}")),
    }
    match systools::schedule::status() {
        Ok(s) => println!("limpeza agendada: {}", s.enabled),
        Err(e) => failures.push(format!("agendamento: {e}")),
    }

    // O runner roda como administrador: o log de boot deve ser legível.
    match systools::boot::run() {
        Ok(b) => println!("boots registrados: {} | itens lentos: {}", b.boots.len(), b.slow.len()),
        Err(e) => println!("boot (não fatal): {e}"),
    }

    match systools::startup::list() {
        Ok(list) => println!("programas na inicialização: {}", list.len()),
        Err(e) => failures.push(format!("inicialização: {e}")),
    }

    if failures.is_empty() {
        println!("OK");
        ExitCode::SUCCESS
    } else {
        for f in &failures {
            eprintln!("FALHA: {f}");
        }
        ExitCode::FAILURE
    }
}
