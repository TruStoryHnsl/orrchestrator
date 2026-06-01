// Self-probe: run detect_system against localhost (this machine) and dump the
// detected hardware so the verifier can sanity-check it against nvidia-smi/free.
use orrch_hwfit::detect_system;

fn main() {
    let s = detect_system("", "", "linux", true);
    println!("total_ram_gb     = {}", s.total_ram_gb);
    println!("available_ram_gb = {}", s.available_ram_gb);
    println!("cpu_cores        = {}", s.cpu_cores);
    println!("cpu_name         = {}", s.cpu_name);
    println!("has_gpu          = {}", s.has_gpu);
    println!("gpu_name         = {:?}", s.gpu_name);
    println!("gpu_vram_gb      = {:?}", s.gpu_vram_gb);
    println!("gpu_count        = {}", s.gpu_count);
    println!("backend          = {}", s.backend);
    println!("gpu_error        = {:?}", s.gpu_error);
    println!("error            = {:?}", s.error);
}
