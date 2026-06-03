use anyhow::Result;

fn main() -> Result<()> {
    let portal = orrch_voice::portal::Portal::from_env()?;
    let reply = portal.send("say hello in five words")?;
    println!("{reply}");
    Ok(())
}
