# Spec de gaps do rendering engine

Levantamento real (código lido, não memória) do estado de `css`, `layout-engine`, `render`, `webgl`, `dom` — 2026-08-23. Cada item vem de leitura direta do source, com `arquivo:linha` quando relevante. Complementa `browser-idle-spec.md` (que cobre a arquitetura geral) focando especificamente no motor de renderização.

Convenção: **REAL** = implementado e testado. **FALTANDO** = confirmado ausente (não parcial, não "quase lá").

## 1. CSS (`crates/css`)

### Seletores (`parser.rs`)
- REAL: tipo (`div`), id (`#foo`), classe (`.foo`), universal (`*`) — `parser.rs:420-452`
- REAL: combinador descendente (espaço) — `parser.rs:405-418`
- **Fechado desde então (commit `dd8467c`, não datado neste doc originalmente — corrigido 2026-08-25):** combinador filho `>`, combinadores irmão `+`/`~` (encadeáveis), seletores de atributo (`[attr]`/`[attr=val]`), e pseudo-classes estruturais `:first-child`/`:last-child`/`:nth-child()` (inteiro, `odd`/`even`, e a fórmula geral `An+B`) — todos reais e testados (`crates/css/tests/selectors_test.rs`). `:hover`/`:focus` parseiam (contam pra especificidade) mas nunca casam — documentado, já que não existe estado de interação nenhum pra eles refletirem.
- FALTANDO: `:not()`
- FALTANDO: pseudo-elementos (`::before`, `::after`)

### At-rules
- REAL: `@import` (URL + media condicional opcional) — `parser.rs:247-288`
- REAL: `@media` (só `min-width`/`max-width`/`width` em px, AND, media type `screen`/`all`) — `parser.rs:294-391`
- FALTANDO: `@font-face`
- FALTANDO: `@keyframes` (e portanto qualquer animação CSS)
- FALTANDO: `@supports`
- FALTANDO: `@charset`
- Todo at-rule não listado acima é reconhecido e descartado (`skip_at_rule_body`, `parser.rs:212-238`) sem erro, mas sem efeito

### Valores
- REAL: `px`, `%`
- FALTANDO: `em`/`rem`/`vh`/`vw`/`vmin`/`vmax`/`pt`/`cm`/... — só existe `Dimension`(px) e `Percentage`
- FALTANDO: custom properties (`--foo`, `var(--foo)`) — `--foo` até tokeniza como `Ident`, mas nada em `style.rs` trata `--*` nem resolve `var()`
- FALTANDO: `calc()` — `parse_length` (`style.rs:171-179`) só aceita um único token, não uma expressão

## 2. Layout (`crates/layout-engine`)

### `ComputedStyle` (`style.rs:122-147`)
Campos reais: `display`, `width`, `height`, `margin`, `padding`, `flex_direction`, `justify_content`, `align_items`, `flex_grow`, `flex_shrink`, `flex_basis`, `background_color`, `font_size`, `color`.

- FALTANDO: `position` (static/relative/absolute/fixed) — não é campo, não é referenciado em `layout.rs`
- FALTANDO: floats (`float`/`clear`)
- FALTANDO: margin collapsing — margens adjacentes sempre somam, nunca colapsam; `margin: auto` vira `0`, não centraliza
- FALTANDO: `border-*` (width/color/style/radius) — border não é modelado, então nem pode ter borda visual
- FALTANDO: `opacity`
- FALTANDO: `z-index` / stacking context
- FALTANDO: `overflow` (ver seção Scroll)

### Inheritance
- REAL: só `font-size` e `color` são herdados (`resolve_style`, `style.rs:377-391`)
- FALTANDO: qualquer outra propriedade herdável por padrão (CSS real herda `visibility`, `cursor`, `line-height`, etc.)

### Flexbox (`flex.rs`)
- REAL: single-line, `flex-direction` (row/column), `justify-content` (start/end/center/space-between/space-around), `align-items` (start/end/center/stretch), `flex-grow`/`flex-shrink` (distribuição ponderada)
- FALTANDO: `flex-wrap` (nunca quebra linha)
- FALTANDO: `order`
- FALTANDO: `align-self`
- FALTANDO: `align-content`
- FALTANDO: `gap`/`row-gap`/`column-gap`
- FALTANDO: `flex-basis: auto` baseado em conteúdo (cai pra width/height, depois `0` — não mede o conteúdo real)

### Grid
- FALTANDO: CSS Grid inteiro — nenhum código em `layout-engine` cobre `display: grid`

### Formatação inline/bloco (`tree.rs`)
- REAL: contexto de formatação inline de verdade — texto + elementos `display: inline` consecutivos viram um `LayoutBox::inline_spans` shapeado junto via `cosmic-text` (`tree.rs:110-262`)
- FALTANDO: geração de anonymous block box — um elemento block-level dentro de conteúdo inline não é "de-inlinado" como um browser real faz (`tree.rs:22-24`)
- FALTANDO: UA stylesheet padrão por tag — `display: inline` só se aplica se a stylesheet disser explicitamente (`b, span { display: inline }`); sem isso, tudo renderiza como bloco

## 3. Render (`crates/render`)

### Pintura (`display_list.rs`, `gpu.rs`)
- REAL: retângulos sólidos com cor de fundo, um draw call, alpha blending — `display_list.rs:29-42`, `gpu.rs:254-300`
- FALTANDO: `border-radius`
- FALTANDO: `border-width`/`border-color` (não modelado em layout, então nem chega no render)
- FALTANDO: `box-shadow`
- FALTANDO: `opacity`/transform (`translate`/`scale`/`rotate`)
- FALTANDO: `clip-path`
- FALTANDO: imagens (ver seção 5)

### Canvas2D (`canvas.rs`)
- REAL: `fillRect`, `clearRect`, `getImageData` — sobre uma textura persistente acumulativa
- FALTANDO: `strokeRect`
- FALTANDO: paths (`beginPath`/`moveTo`/`lineTo`/`arc`/`closePath`/`fill`/`stroke`)
- FALTANDO: texto (`fillText`/`strokeText`/`measureText`)
- FALTANDO: `drawImage`
- FALTANDO: gradientes (`createLinearGradient`/`createRadialGradient`) e patterns
- FALTANDO: transforms (`translate`/`rotate`/`scale`/`setTransform`)
- FALTANDO: modos de composição além do fill (alpha blend) / clear (replace)

## 4. WebGL (`crates/webgl`)

- REAL: GLSL ES 3.00/3.10/3.20 via naga (rewrite do `#version ... es` pra `#version 450 core`) — `context.rs:64-88`
- REAL: `create_shader`/`create_program`/`draw_triangles` — pipeline real, draw + readback headless
- FALTANDO: texturas (nenhum tipo/binding existe)
- FALTANDO: uniforms
- FALTANDO: indexed drawing (`drawElements`)
- FALTANDO: framebuffers
- FALTANDO: VAOs / transform feedback
- FALTANDO: extensions
- FALTANDO: qualquer topologia além de `TRIANGLES`
- FALTANDO: estado persistente entre draw calls (cada `draw_triangles` é auto-contido, sem conceito de "programa atual"/VAO ligado)
- FALTANDO: GLSL ES 1.00 (WebGL1) — gramática `attribute`/`varying` diferente demais pro rewrite de uma linha cobrir

## 5. Imagens

**Fechado (2026-08-24), real, ponta a ponta.** `image_decode` (PNG+JPEG via a crate `image`), `layout_engine::apply_image_sizes` (sizing intrínseco real) e `render::build_image_list`/`composite_images` (compositing real, nearest-neighbor) já existiam e eram testados isoladamente por crate — mas nada em `profile-worker`, o único processo que realmente renderiza uma página navegada, jamais chamava nenhum dos três. Uma página real com `<img>` continuava sem pintar nada, apesar de cada peça individual já ser real. Fechado agora: `profile-worker`'s `load_images`/`collect_image_sources` buscam (via `fetch_with_cookies` — mesmo proxy/DNS/cookie jar de qualquer outro fetch deste worker) e decodificam cada `<img src>` no carregamento da página (mesma convenção "um GET real por recurso, no load" que `build_stylesheet` já usa pra `<link>`), e `Page::render`/`hit_test_at` agora compartilham um `Page::layout` único que aplica `apply_image_sizes` antes do layout — antes cada um reconstruía sua própria árvore de boxes de forma independente, o que teria discordado silenciosamente assim que o sizing intrínseco de imagem entrasse em cena.

- FALTANDO: `HTMLImageElement` como tipo de nó dedicado (`onload`/`onerror`/`naturalWidth`/`naturalHeight` em JS) — `dom::NodeData::Element` continua genérico, sem hierarquia de classe por tag
- FALTANDO: GIF/WebP/AVIF/SVG (a feature-set da crate `image` habilitada aqui é só PNG+JPEG), imagens animadas (só o primeiro frame), perfil de cor ICC
- FALTANDO: `srcset`/`<picture>`/`loading="lazy"`
- Uma falha de fetch/decode (URL não resolve, 404, bytes corrompidos) deixa a box vazia (mesmo comportamento de um `<img>` sem `src`), sem erro reportado

## 6. Input real (mouse/teclado → DOM)

**Fechado (2026-08-23), real: `layout_engine::hit_test` (coordenada → `NodeId` real sobre a `LayoutBox` tree já laid-out), `profile-worker`'s `CLICK_AT <x> <y>`/`KEY <key>` commands, `Profile::click_at`/`type_key`, e `apps/shell` roteando clique/teclado reais do egui pra dentro da página renderizada (não só selecionar o pane). Limitação real herdada do `CLICK`/`FILL` já existente: só alcança elemento com `id` (direto ou via ancestral) — sem isso não tem como despachar. Sem foco/tab order real (`activeElement`), sem `.value` real (ver seção 8) — `KEY` escreve em `textContent`.**

Bug real encontrado e corrigido nesse processo: `getElementById` criava um objeto JS novo a cada chamada, então um listener anexado numa chamada `eval()` ficava invisível pra um `dispatchEvent` de uma chamada `eval()` separada depois — quebrava o padrão "anexa listener no load, despacha depois" que qualquer página real usa. Corrigido com identidade de objeto real por `NodeId` (cache thread-local).

Também descoberto e corrigido: páginas navegadas nunca executavam suas próprias tags `<script>` (só a página demo hardcoded rodava). E um bug real e severo no parser CSS: seletor com pseudo-classe não suportada (ex: `:link`, que `https://example.com/` usa de verdade) travava o parser em loop infinito pra sempre — qualquer página real com CSS fora do subset suportado travava o worker inteiro.

O que existia antes desse fechamento, pra contexto histórico:
- `Profile::click(selector)`/`Profile::fill(selector, value)` — só `#id`, dirigido via protocolo stdin `CLICK`/`FILL` do `profile-worker`, usado por scripts de automação (`pane.click(...)`), não pelo usuário clicando na tela
- `CLICK` dispara um evento `"click"` real via `dom::Dom::find_by_id` + `dispatchEvent` já existente
- `FILL` seta `textContent` (não existe `HTMLInputElement.value`, ver seção 8)

O que falta:
- **Hit-testing por coordenada**: nenhum código traduz a posição de um clique (pixel x,y dentro da textura de um pane) pra qual `LayoutBox`/nó DOM está ali. Clicar na textura de um pane em `apps/shell` só seleciona aquele pane (`main.rs:1164-1165`), nunca chega no conteúdo renderizado
- **Foco/tab order**: `dom::NodeData` não tem estado de foco, `tabindex`, `focus()`, `activeElement` — nada disso existe em `dom` ou `js-runtime`
- **Teclado real**: nenhuma tecla digitada pelo usuário chega em um elemento focado — só existe `FILL` via automação

Consequência prática: um humano não consegue clicar num link ou digitar num campo interagindo com os pixels renderizados. Só é possível via automação `pane.click`/`pane.fill` por id.

## 7. Scroll

**Parcialmente fechado (2026-08-25), real: scroll de viewport (a página inteira, não containers internos com `overflow`).** `profile-worker` ganhou um comando `SCROLL <dy>` real (`crates/profile/src/bin/profile_worker.rs`): um `scroll_top: f64` por página (resetado a `0` em `RELOAD`/`NAVIGATE`, igual `focused_id`), clampado em `[0, content_height - viewport_height]` via `Page::content_height` (a altura real do box raiz já laid-out — layout em si sempre calcula o conteúdo inteiro, sem cortar no viewport; scroll é só uma janela de pintura sobre isso). `Page::render` desloca cada `Rect`/`ImageQuad`/`PositionedGlyph` já pintado por `-scroll_top` antes de compositar (os três compositors — `render_to_rgba`/`composite_images`/`composite_glyphs` — já clipavam silenciosamente qualquer coisa fora de `[0, height)`, então não precisou de clipping novo); `Page::hit_test_at` soma `scroll_top` de volta em `y` antes de testar, então `CLICK_AT` continua acertando o elemento real sob o cursor mesmo com a página rolada. `Page::render`/`hit_test_at` agora passam por um `Page::layout` único (mesmo que a sessão anterior já unificou pra imagens), garantindo que pintura e hit-test concordam sobre a mesma árvore. `profile::Profile::scroll_by(dy)` expõe isso pro host; `apps/shell`'s grade de panes agora rota o scroll real do mouse (`egui`'s `raw_scroll_delta`, sinal invertido pra bater com a convenção do protocolo) pro pane que está sob o cursor (`hovered()`, não foco — igual um browser real rola o que está sob o mouse).

- FALTANDO: `overflow` como propriedade de `ComputedStyle` — scroll continua sendo só do viewport/documento inteiro, um `<div style="overflow:auto">` interno não tem scroll próprio (precisaria virar sua própria "janela de pintura" recursiva, não só uma no nível da página)
- FALTANDO: barra de rolagem visual (nenhum indicador de posição/tamanho de scroll é desenhado)
- FALTANDO: `scrollTop`/`scrollLeft`/`scrollIntoView`/`scroll()` como API JS em `document`/`Node` — o scroll só existe no lado do host (protocolo `SCROLL`), nada em `js-runtime` expõe isso pra um script da própria página ler ou setar
- FALTANDO: scroll horizontal — o protocolo e o clamping só cobrem o eixo vertical, e o box model deste engine não tem noção de conteúdo mais largo que o container de qualquer forma
- FALTANDO: scroll suave/momentum — cada `SCROLL` aplica o delta e repinta imediatamente, sem animação

## 8. Formulários / elementos de input

**Parcialmente fechado (2026-08-24), real:** `.value` real e independente de `textContent` (`dom::Dom::value`/`set_value`, novo campo `value: Option<String>` em `NodeData::Element`) e foco real (`dom::Dom::focus`/`blur`/`clear_focus`/`active_element`) — expostos em `js-runtime` como `Node.prototype.value` (getter/setter), `Node.prototype.focus()`/`blur()`, e `document.activeElement` (`crates/js-runtime/src/dom_bindings.rs`). `<input value="...">` (o atributo HTML) semeia `.value` via `Dom::set_attribute`'s mirroring; `<textarea>` cai pro seu `textContent` real até `.value` ser tocado, depois vira independente — o mesmo comportamento real do DOM, simplificado (sem "dirty value flag" por trás: `set_attribute("value", ...)` sempre espelha, não só antes da primeira interação). `profile-worker`'s `FILL`/`KEY` agora escrevem `.value` de verdade num `<input>`/`<textarea>` (`textContent` continua sendo o alvo pra qualquer outro elemento), e `CLICK_AT` agora foca de verdade via `dom::Dom::focus`/`clear_focus` (não só um tracker local de string) — `document.activeElement` reflete um clique real do usuário, não só chamadas JS.

- FALTANDO ainda: tipo de nó dedicado pra `<input>`/`<textarea>`/`<select>` — `dom::NodeData::Element` continua genérico (`{ tag, attributes, value }`), sem distinção por tag; `.value` é exposto genericamente em `Node.prototype`, não numa hierarquia `HTMLInputElement`/`HTMLTextAreaElement` própria (documentado no código)
- FALTANDO: `<select>`/`<option>` — nenhum tratamento especial, `.value` genérico não modela `selectedIndex`/opções
- FALTANDO: tab order (`Tab`/`Shift+Tab` movendo foco), `tabindex` — só existe foco disparado por clique real (`CLICK_AT`) ou chamada JS explícita (`.focus()`/`.blur()`)
- FALTANDO: eventos `focus`/`blur`/`input`/`change` reais disparados como `Event` — `focus()`/`blur()` mudam o estado real mas não despacham nada via `dispatchEvent`; só `"keydown"` é disparado (por `KEY`, já existia antes)

## Resumo de prioridade (se fosse continuar o motor)

Ordenado por "o que mais destrava uso real da engine":

1. **Input real (hit-testing + foco + teclado)** — sem isso, ninguém usa o browser como usuário, só como executor de scripts
2. **Elementos de formulário reais (`value`, foco)** — pré-requisito pro item 1 fazer sentido em páginas com login/busca
3. **Imagens** — a maioria das páginas reais tem pelo menos um `<img>`; sem isso qualquer página parece quebrada
4. **Scroll** — páginas mais altas que o viewport hoje simplesmente cortam sem aviso
5. **`position`/floats** — muito layout real depende disso, mas menos crítico que os 4 acima pra "consigo navegar"
6. **Seletores CSS mais completos** (`>`, pseudo-classes) — cosmético/compat, não bloqueia uso básico
7. **Bordas/box-shadow/opacity** — puramente visual, última prioridade
