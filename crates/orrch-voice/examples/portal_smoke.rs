use anyhow::Result;

fn main() -> Result<()> {
    let portal = orrch_voice::portal::portal_agent_from_env()?;
    let turn = portal.send_turn("say hello in five words")?;
    println!("{}", turn.reply);
    Ok(())
}
