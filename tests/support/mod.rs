use std::{
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::{Command, Output},
    thread,
};

use tempfile::TempDir;
use trust::{
    ai::AiConfig,
    generator::CodeGen,
    lexer::Lexer,
    parser::{Ast, Parser},
};

/// A freshly generated project living in its own temp directory (dropped —
/// and the directory removed — at the end of the test, including on panic).
#[derive(Debug)]
pub struct GeneratedProject {
    _dir: TempDir,
    pub root: PathBuf,
}

impl GeneratedProject {
    /// Reads a generated file's contents. `relative` is relative to the
    /// project root (e.g. `"src/applications/create_task_use_case.rs"`).
    pub fn read(&self, relative: &str) -> String {
        let path = self.root.join(relative);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {:?}: {}", path, e))
    }

    pub fn exists(&self, relative: &str) -> bool {
        self.root.join(relative).exists()
    }
}

pub fn parse(src: &str) -> Vec<Ast> {
    try_parse(src).expect("lex/parse should succeed")
}

pub fn try_parse(src: &str) -> Result<Vec<Ast>, String> {
    let tokens = Lexer::new(&src.to_string()).run()?;
    Parser::new(tokens).run()
}

fn try_generate_with(src: &str, ai_config: &AiConfig) -> Result<GeneratedProject, String> {
    let asts = try_parse(src)?;
    let dir = TempDir::new().expect("failed to create tempdir");
    let root = dir.path().join("out");

    CodeGen::new(asts).run(root.clone(), false, ai_config)?;
    Ok(GeneratedProject { _dir: dir, root })
}

/// Generates a project from `.tr` source into a fresh temp directory,
/// in-process (no subprocess, no real `cargo build` — that's e2e's job).
/// Never talks to a real AI provider: `ai {}` use cases would need a mock
/// server, out of scope for source-shape assertions (use
/// `try_generate_with_ai` for those).
pub fn try_generate(src: &str) -> Result<GeneratedProject, String> {
    try_generate_with(src, &AiConfig::new_for_testing("http://127.0.0.1:1"))
}

pub fn generate(src: &str) -> GeneratedProject {
    try_generate(src).unwrap_or_else(|e| panic!("code generation should succeed: {}", e))
}

pub fn try_generate_with_ai(src: &str, ai_config: &AiConfig) -> Result<GeneratedProject, String> {
    try_generate_with(src, ai_config)
}

pub fn generate_with_ai(src: &str, ai_config: &AiConfig) -> GeneratedProject {
    try_generate_with_ai(src, ai_config).unwrap_or_else(|e| panic!("code generation should succeed: {}", e))
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// A one-shot local HTTP server standing in for the real Anthropic API:
/// accepts exactly one connection, reads the full request, and replies with
/// a fixed, always-valid Anthropic-shaped JSON response wrapping
/// `response_text` (the literal Rust statements an `ai {}` use case's body
/// becomes). Lets `ai {}` codegen — and, in e2e tests, real compilation of
/// its output — be tested without ever calling a real AI provider.
pub fn mock_anthropic_returning(response_text: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind mock server");
    let addr = listener.local_addr().expect("failed to read mock server address");
    let body = serde_json::json!({ "content": [{ "text": response_text }] }).to_string();

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("mock server failed to accept");

        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        let header_end = loop {
            let n = stream.read(&mut chunk).expect("mock server failed to read request");
            buf.extend_from_slice(&chunk[..n]);
            if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
                break pos + 4;
            }
        };
        let headers = String::from_utf8_lossy(&buf[..header_end]).to_string();
        let content_length: usize = headers
            .lines()
            .find_map(|line| line.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().to_string()))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        while buf.len() - header_end < content_length {
            let n = stream.read(&mut chunk).expect("mock server failed to read body");
            buf.extend_from_slice(&chunk[..n]);
        }

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body,
        );
        stream.write_all(response.as_bytes()).expect("mock server failed to write response");
    });

    format!("http://{}", addr)
}

/// The trust repo's own root — where `mod/common`, `mod/auth`, etc. live —
/// so e2e tests can patch a generated project's git dependencies to those
/// local checkouts instead of fetching them over the network.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every private git dependency this generator can emit, and the local
/// checkout each one should be patched to for e2e `cargo build`s. Only
/// entries actually present in a given project's `Cargo.toml` get patched in
/// — most fixtures only ever pull in `common`.
const LOCAL_GIT_DEPENDENCIES: &[(&str, &str)] = &[
    ("common", "ssh://git@github.com/BH-World/common.git"),
    ("auth", "ssh://git@github.com/BH-World/auth.git"),
    ("user_permission", "ssh://git@github.com/BH-World/user_permission.git"),
];

/// Appends `[patch."<git url>"]` blocks redirecting any of this generated
/// workspace's private git dependencies to their local `mod/*` checkouts, so
/// `cargo build` never needs network/SSH access to a private repo. Each
/// named `app` is its own member crate with its own `Cargo.toml`, so every
/// member manifest is scanned for the git URL — but Cargo only honors
/// `[patch]` at the workspace root, so the resulting block is always
/// appended to the root manifest, never a member's.
pub fn patch_local_dependencies(project: &GeneratedProject) {
    let root_cargo_toml_path = project.root.join("Cargo.toml");
    let member_manifests: Vec<PathBuf> = std::fs::read_dir(&project.root)
        .unwrap_or_else(|e| panic!("failed to read workspace root {:?}: {}", project.root, e))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().join("Cargo.toml"))
        .filter(|path| path.is_file())
        .collect();

    let mut patch_block = String::new();
    for (name, git_url) in LOCAL_GIT_DEPENDENCIES {
        let referenced = member_manifests.iter().any(|manifest| {
            std::fs::read_to_string(manifest)
                .unwrap_or_else(|e| panic!("failed to read {:?}: {}", manifest, e))
                .contains(git_url)
        });
        if referenced {
            let local_path = repo_root().join("mod").join(name);
            patch_block.push_str(&format!(
                "\n[patch.\"{}\"]\n{} = {{ path = {:?} }}\n",
                git_url, name, local_path
            ));
        }
    }

    if !patch_block.is_empty() {
        let mut content = std::fs::read_to_string(&root_cargo_toml_path)
            .unwrap_or_else(|e| panic!("failed to read workspace Cargo.toml: {}", e));
        content.push_str(&patch_block);
        std::fs::write(&root_cargo_toml_path, content)
            .unwrap_or_else(|e| panic!("failed to write patched Cargo.toml: {}", e));
    }
}

/// Runs a real `cargo build` (offline — every dependency this workspace
/// needs is already vendored in the local registry cache/`mod/*` checkouts)
/// against the generated project's own `Cargo.toml`. Shares one
/// `CARGO_TARGET_DIR` across every e2e test (rather than each generated
/// project getting its own inside its own temp dir) so common dependencies
/// (axum, tokio, sqlx, ...) are only ever compiled once, not once per test.
pub fn cargo_build(project: &GeneratedProject) -> Output {
    Command::new("cargo")
        .args(["build", "--offline", "--manifest-path"])
        .arg(project.root.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", repo_root().join("target/e2e-tests"))
        .output()
        .expect("failed to spawn cargo build")
}

pub fn assert_build_succeeds(project: &GeneratedProject) {
    let output = cargo_build(project);
    assert!(
        output.status.success(),
        "cargo build failed:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
