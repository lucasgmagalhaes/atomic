# IdleBrowser — Spec técnica de execução

## Requisitos
- Plataformas: Windows, Linux, macOS
- Escala: até 5 contas/abas simultâneas
- Isolamento: sessão/cookies separados por conta (sem fingerprint/IP spoofing, sem proxy)

## Decisões de arquitetura

| Camada | Escolha | Motivo |
|---|---|---|
| Motor de renderização | Do zero — sem CEF/Chromium/Ultralight/Tauri | leveza + controle total + consistência cross-platform |
| JS | QuickJS via FFI C↔Rust | maduro, rápido, spec ECMAScript quase completa |
| Rede/TLS | `hyper` + `rustls` | única exceção ao "do zero" — TLS próprio é risco de segurança injustificável |
| Parser HTML | `html5ever` | evita reimplementar error-recovery da spec HTML5 |
| Renderer | `wgpu` | abstrai Vulkan/Metal/DX12, cross-platform nativo |
| Texto | `cosmic-text` | shaping + rasterização prontos |
| UI da shell | `egui` + `eframe` | immediate-mode, fácil pra desenho custom (tiling) |
| Isolamento de conta | 1 processo de SO por perfil + IPC | crash isolation + security boundary — **risco:** 5 stacks completos (parser+layout+JS+render) em paralelo = footprint de RAM alto, tensiona com meta de "leveza"; medir consumo real na fase 4 antes de assumir viável |
| Comunicação shell↔perfil | Frame em shared memory (blit) + eventos de input + comandos | simples de implementar, GPU handle direto é otimização futura — **pendente:** definir sincronização (double buffer ou lock) entre escrita do perfil e leitura da shell pra evitar tearing; detalhar na fase 4 |
| Layout da shell | Árvore BSP por grupo — modo automático (recalcula em grid ao abrir/fechar) + manual (drag = muda ratio/divide nó) | atende auto+manual sem duas implementações separadas |

## APIs Web no escopo

**Bloqueante (fases 2-4):** localStorage, sessionStorage, IndexedDB, Page Visibility, Web Workers, requestAnimationFrame, Notifications, performance.now(), Canvas 2D, Web Audio, Clipboard, crypto.getRandomValues, fetch/XHR, File/Blob

**Sob demanda (fase 6):** Screen Wake Lock, formatação de números grandes (impl própria, não ICU), ResizeObserver, IntersectionObserver, Service Worker/PWA, Gamepad API, MutationObserver

## Estrutura de diretórios

```bash
mkdir -p idlebrowser/apps/shell/src/{ui,tiling,workspace}
mkdir -p idlebrowser/crates/{net,html,css,layout-engine,dom,render,webgl,storage,workers,platform-apis,profile,ipc,security}/src
mkdir -p idlebrowser/crates/js-runtime/src idlebrowser/crates/js-runtime/quickjs-sys/src
mkdir -p idlebrowser/xtask/src
```

## `Cargo.toml` (workspace root)

```toml
[workspace]
resolver = "2"
members = [
    "apps/shell",
    "crates/net", "crates/html", "crates/css", "crates/layout-engine",
    "crates/dom", "crates/js-runtime", "crates/js-runtime/quickjs-sys",
    "crates/render", "crates/webgl", "crates/storage", "crates/workers",
    "crates/platform-apis", "crates/profile", "crates/ipc", "crates/security",
    "xtask",
]
```

## Crates — dependências, responsabilidade, fase

| Crate | Dependências principais | Responsabilidade | Fase de implementação |
|---|---|---|---|
| apps/shell | egui, eframe | UI chrome, tiling BSP, orquestração | 1 = stub · 4 = completo |
| net | hyper, rustls, tokio, tokio-tungstenite | HTTP/TLS/WebSocket | 1 |
| html | html5ever | parser HTML | 1 |
| dom | — | árvore DOM, eventos | **1 = implementar agora** |
| layout-engine | — | box tree, flex, positioning | 1 = stub · 3 = completo |
| css | — | parse + cascade | 3 |
| render | wgpu | compositor | 1 = stub · 3 = completo |
| js-runtime | cc (build-dep) | bindings DOM↔JS | 2 |
| js-runtime/quickjs-sys | cc (build-dep) | FFI bruto do QuickJS | 2 |
| webgl | — | API WebGL → wgpu | 3 |
| storage | — | cookies/localStorage/sessionStorage/IndexedDB | 4 |
| workers | — | Web Workers | 4 |
| platform-apis | — | Page Visibility, rAF, Notifications, performance.now, Clipboard, crypto, File/Blob, Wake Lock, ResizeObserver/IntersectionObserver | 2/4/6 |
| profile | — | processo isolado por conta (host de dom+js-runtime+render+webgl) | 4 |
| ipc | — | protocolo shell↔profile (frame + input + comandos) | 4 |
| security | — | sandbox por processo, criptografia de sessão em disco | 5 |

## Tarefa desta sessão — Fase 1

Escopo: **só `dom` recebe implementação real.** Demais crates ficam como stub que compila (`Cargo.toml` + `lib.rs` vazio ou `pub fn placeholder() {}`).

### T1 — Scaffold
O quê: criar todos os crates/app listados.
Como: comandos `mkdir` acima + `cargo init --lib` em cada crate, `cargo init --bin` em `apps/shell` e `xtask`.

### T2 — `crates/dom` (implementação real)
O quê: árvore DOM arena-based (não usar `Rc<RefCell<>>` — mais lento e mais difícil de tornar `Send` depois pra cruzar threads/IPC).

Decisões corrigidas nesta revisão:
- `NodeId` com **generation tag** (não `usize` cru). Sem isso, remoção de nó (necessária já na fase 2 pra `innerHTML =`, mutação DOM) causa reuso de índice e aliasing silencioso — caro de retrofitar depois que JS bindings dependerem do tipo cru.
- `remove_child` incluído — sem ele, reparent via `append_child` duplica o filho em dois `children` (nó órfão referenciado por dois pais).
- `get` retorna `Option<&Node>` — indexação direta (`&self.nodes[id]`) panica em id inválido/obsoleto; arena com free-list garante que isso aconteça.

Como — `crates/dom/src/lib.rs`:
```rust
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId {
    index: usize,
    generation: u32,
}

#[derive(Debug, Clone)]
pub enum NodeData {
    Document,
    Element { tag: String, attributes: HashMap<String, String> },
    Text(String),
    Comment(String),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: NodeId,
    pub data: NodeData,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

struct Slot {
    generation: u32,
    node: Option<Node>,
}

pub struct Dom {
    slots: Vec<Slot>,
    free: Vec<usize>,
    root: NodeId,
}

impl Dom {
    pub fn new() -> Self {
        let root_id = NodeId { index: 0, generation: 0 };
        let root = Node { id: root_id, data: NodeData::Document, parent: None, children: vec![] };
        Dom { slots: vec![Slot { generation: 0, node: Some(root) }], free: vec![], root: root_id }
    }

    fn insert(&mut self, data: NodeData) -> NodeId {
        if let Some(index) = self.free.pop() {
            let generation = self.slots[index].generation;
            let id = NodeId { index, generation };
            self.slots[index].node = Some(Node { id, data, parent: None, children: vec![] });
            id
        } else {
            let index = self.slots.len();
            let id = NodeId { index, generation: 0 };
            self.slots.push(Slot { generation: 0, node: Some(Node { id, data, parent: None, children: vec![] }) });
            id
        }
    }

    pub fn create_element(&mut self, tag: &str) -> NodeId {
        self.insert(NodeData::Element { tag: tag.to_string(), attributes: HashMap::new() })
    }

    pub fn create_text(&mut self, text: &str) -> NodeId {
        self.insert(NodeData::Text(text.to_string()))
    }

    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        self.detach(child);
        if let Some(node) = self.get_mut(child) { node.parent = Some(parent); }
        if let Some(node) = self.get_mut(parent) { node.children.push(child); }
    }

    /// Remove `child` from its current parent's children list without freeing it.
    fn detach(&mut self, child: NodeId) {
        let old_parent = self.get(child).and_then(|n| n.parent);
        if let Some(old_parent) = old_parent {
            if let Some(node) = self.get_mut(old_parent) {
                node.children.retain(|&c| c != child);
            }
        }
    }

    /// Remove a node and its whole subtree, freeing slots for reuse (generation bumped).
    pub fn remove(&mut self, id: NodeId) {
        let children = self.get(id).map(|n| n.children.clone()).unwrap_or_default();
        for child in children {
            self.remove(child);
        }
        self.detach(id);
        if id.index < self.slots.len() && self.slots[id.index].generation == id.generation {
            self.slots[id.index].node = None;
            self.slots[id.index].generation = self.slots[id.index].generation.wrapping_add(1);
            self.free.push(id.index);
        }
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.slots.get(id.index).and_then(|slot| {
            if slot.generation == id.generation { slot.node.as_ref() } else { None }
        })
    }

    fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.slots.get_mut(id.index).and_then(|slot| {
            if slot.generation == id.generation { slot.node.as_mut() } else { None }
        })
    }

    pub fn root(&self) -> NodeId { self.root }
}
```
Incluir testes unitários: criar elemento, anexar filho, verificar `parent`/`children`; reparent (mover filho entre dois pais, checar que sai da lista antiga); `remove` seguido de `get` no id antigo retornando `None` (valida generation).

### T3 — `apps/shell` (stub)
O quê: janela egui vazia funcional, só pra validar que o app roda nas 3 plataformas.
Como — `apps/shell/Cargo.toml`:
```toml
[package]
name = "shell"
edition = "2021"

[dependencies]
eframe = "0.28"
egui = "0.28"
```
Checar versão atual de `eframe`/`egui` no `cargo add` antes do scaffold — spec fixa 0.28, pode já estar desatualizada.
`apps/shell/src/main.rs`:
```rust
fn main() -> eframe::Result<()> {
    eframe::run_simple_native("IdleBrowser", eframe::NativeOptions::default(), move |ctx, _frame| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("IdleBrowser — shell stub (fase 1)");
        });
    })
}
```

### T4 — `.gitignore`
```
/target
```
(Cargo.lock deve ser versionado — é um binário, não uma lib pura.)

### T5 — `README.md`
O quê: resumo do projeto (objetivo, arquitetura, roadmap) pra quem abrir o repo depois.
Como: gerar a partir das seções "Requisitos", "Decisões de arquitetura" e "Roadmap" deste documento, resumidas.

### T6 — Validação
`cargo build --workspace` precisa compilar sem erro ao final. `cargo test -p dom` precisa passar.

## Fora de escopo nesta sessão

Não implementar: QuickJS FFI real, parsing HTML/CSS real, renderer wgpu real, IPC real, tiling BSP real. Só estrutura + `dom` funcional.

## Roadmap — fases seguintes

| Fase | Entregável |
|---|---|
| 2 | QuickJS embutido + bindings mínimos (getElementById, textContent, addEventListener, timers, rAF, performance.now, Page Visibility, crypto.getRandomValues) |
| 3 | CSS (flex/positioning) + Canvas 2D + WebGL |
| 4 | Storage completo + Workers + isolamento de processo (profile/ipc) + shell completa + Notifications + keep-alive + Web Audio + Clipboard + fetch/XHR + File/Blob |
| 5 | Hardening: sandbox por processo (mecanismo por plataforma — Windows: AppContainer/Job Objects · Linux: seccomp+namespaces · macOS: App Sandbox; 3 implementações distintas, não 1 item), criptografia de sessão em disco, updater assinado |
| 6 | Sob demanda: Wake Lock, formatação de números, ResizeObserver/IntersectionObserver, Service Worker/PWA, Gamepad, MutationObserver |
