use anyhow::Result;
use orrch_voice::portal::PortalAgent;
use orrch_voice::portal_local::OllamaPortal;

fn main() -> Result<()> {
    let portal = OllamaPortal::from_ollama_url("llama3:8b", "http://localhost:11434")?;
    let turn = portal.send_turn("Say hello in five words.")?;
    println!("{}", turn.reply);
    Ok(())
}
