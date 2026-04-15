use std::path::{Path, PathBuf};
use crate::item::{ItemKind, LibraryItem};
use crate::store::{parse_frontmatter_pub, extract_field_pub};

/// Load all PI extensions from `dir` (`library/pi-extensions/*.ts`).
/// Returns a `LibraryItem` per file; description comes from the first
/// comment line of the form `// Description: ...`.
pub fn load_pi_extensions(dir: &Path) -> Vec<LibraryItem> {
    let mut items = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return items;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "ts") {
            if let Some(item) = load_pi_extension_item(&path) {
                items.push(item);
            }
        }
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    items
}

fn load_pi_extension_item(path: &Path) -> Option<LibraryItem> {
    let content = std::fs::read_to_string(path).ok()?;
    let name = path.file_stem()?.to_string_lossy().to_string();

    // Extract description from first `// Description: ...` comment line.
    let description = content.lines()
        .find_map(|line| {
            let trimmed = line.trim();
            trimmed.strip_prefix("// Description:").map(|s| s.trim().to_string())
        })
        .unwrap_or_default();

    Some(LibraryItem {
        name,
        kind: ItemKind::PiExtension,
        description,
        tags: Vec::new(),
        content: content.clone(),
        path: path.to_path_buf(),
    })
}

/// Translate a skill `.md` file into a PI extension `.ts` file.
/// Extracts frontmatter `name`, `description`, `tags` and wraps the
/// body as a system-prompt injected via `pi.on("session_start")`.
/// Returns the path of the written `.ts` file.
pub fn translate_skill_to_pi_extension(skill_path: &Path, out_dir: &Path) -> anyhow::Result<PathBuf> {
    let content = std::fs::read_to_string(skill_path)?;
    let slug = skill_path
        .file_stem()
        .ok_or_else(|| anyhow::anyhow!("no stem"))?
        .to_string_lossy()
        .to_string();

    let (name, description, body) = if let Some((fm, body)) = parse_frontmatter_pub(&content) {
        let name = extract_field_pub(&fm, "name").unwrap_or_else(|| slug.clone());
        let description = extract_field_pub(&fm, "description").unwrap_or_default();
        (name, description, body.trim().to_string())
    } else {
        (slug.clone(), String::new(), content.trim().to_string())
    };

    let ts = skill_to_ts(&slug, &name, &description, &body, skill_path.to_string_lossy().as_ref());
    std::fs::create_dir_all(out_dir)?;
    let out_path = out_dir.join(format!("{}.ts", slug));
    std::fs::write(&out_path, ts)?;
    Ok(out_path)
}

/// Translate a tool shell script into a PI extension `.ts` file.
/// Wraps the script content in a `registerTool` that shells out via bash.
/// Returns the path of the written `.ts` file.
pub fn translate_tool_to_pi_extension(tool_path: &Path, out_dir: &Path) -> anyhow::Result<PathBuf> {
    let content = std::fs::read_to_string(tool_path)?;
    let slug = tool_path
        .file_stem()
        .ok_or_else(|| anyhow::anyhow!("no stem"))?
        .to_string_lossy()
        .to_string();

    // Extract description from leading comment lines (skip shebang).
    let description = content.lines()
        .skip_while(|l| l.starts_with("#!/"))
        .find_map(|line| {
            let t = line.trim_start_matches('#').trim();
            if t.is_empty() { None } else { Some(t.to_string()) }
        })
        .unwrap_or_else(|| format!("{} tool", slug));

    let ts = tool_to_ts(&slug, &description, &content, tool_path.to_string_lossy().as_ref());
    std::fs::create_dir_all(out_dir)?;
    let out_path = out_dir.join(format!("{}.ts", slug));
    std::fs::write(&out_path, ts)?;
    Ok(out_path)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn skill_to_ts(slug: &str, name: &str, description: &str, body: &str, source: &str) -> String {
    // Escape backticks inside the body for template literal safety.
    let escaped_body = body.replace('\\', "\\\\").replace('`', "\\`").replace("${", "\\${");
    format!(
        r#"import type {{ ExtensionAPI }} from "@mariozechner/pi-coding-agent";

// {name} — orrchestrator PI extension
// Translated from: {source}
// Description: {description}

function getSystemPrompt(): string {{
  return `{escaped_body}`;
}}

export default function (pi: ExtensionAPI) {{
  pi.on("session_start", async (_event, ctx) => {{
    ctx.systemPrompt = (ctx.systemPrompt ?? "") + "\n\n" + getSystemPrompt();
  }});

  // Custom tool
  // pi.registerTool({{ name: "{slug}", description: "{description}", parameters: {{}}, async execute(id, params, signal, onUpdate, ctx) {{ }} }});
}}
"#,
        name = name,
        source = source,
        description = description,
        escaped_body = escaped_body,
        slug = slug,
    )
}

fn tool_to_ts(slug: &str, description: &str, script: &str, source: &str) -> String {
    // Escape the script for embedding in a template literal.
    let escaped_script = script.replace('\\', "\\\\").replace('`', "\\`").replace("${", "\\${");
    format!(
        r#"import type {{ ExtensionAPI }} from "@mariozechner/pi-coding-agent";
import {{ spawnSync }} from "child_process";

// {slug} — orrchestrator PI extension (tool)
// Translated from: {source}
// Description: {description}

const SCRIPT = `{escaped_script}`;

export default function (pi: ExtensionAPI) {{
  pi.registerTool({{
    name: "{slug}",
    description: "{description}",
    parameters: {{
      type: "object",
      properties: {{
        args: {{ type: "string", description: "Arguments to pass to the script" }},
      }},
      required: [],
    }},
    async execute(_id, params: {{ args?: string }}, _signal, onUpdate, _ctx) {{
      const args = params.args ?? "";
      const result = spawnSync("bash", ["-c", SCRIPT + " " + args], {{
        encoding: "utf8",
        timeout: 30_000,
      }});
      const out = result.stdout || result.stderr || (result.error?.message ?? "no output");
      onUpdate(out);
      return out;
    }},
  }});
}}
"#,
        slug = slug,
        source = source,
        description = description,
        escaped_script = escaped_script,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_translate_skill_produces_ts() {
        let dir = tempdir().unwrap();
        let skill = dir.path().join("my_skill.md");
        std::fs::write(&skill, "---\nname: My Skill\ndescription: Does things\n---\n\n# Body\nDo stuff.").unwrap();
        let out_dir = dir.path().join("pi-extensions");
        let out = translate_skill_to_pi_extension(&skill, &out_dir).unwrap();
        assert!(out.exists());
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("getSystemPrompt"));
        assert!(content.contains("My Skill"));
        assert!(content.contains("Does things"));
        assert!(content.contains("Do stuff."));
    }

    #[test]
    fn test_translate_tool_produces_ts() {
        let dir = tempdir().unwrap();
        let tool = dir.path().join("run_thing.sh");
        std::fs::write(&tool, "#!/bin/bash\n# Run the thing\necho hello").unwrap();
        let out_dir = dir.path().join("pi-extensions");
        let out = translate_tool_to_pi_extension(&tool, &out_dir).unwrap();
        assert!(out.exists());
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("registerTool"));
        assert!(content.contains("run_thing"));
    }

    #[test]
    fn test_load_pi_extensions_empty_dir() {
        let dir = tempdir().unwrap();
        let items = load_pi_extensions(dir.path());
        assert!(items.is_empty());
    }

    #[test]
    fn test_load_pi_extensions_reads_ts() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("ext.ts"), "// Description: hello\nexport default function(pi) {}").unwrap();
        let items = load_pi_extensions(dir.path());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "ext");
        assert_eq!(items[0].description, "hello");
        assert_eq!(items[0].kind, crate::item::ItemKind::PiExtension);
    }
}
