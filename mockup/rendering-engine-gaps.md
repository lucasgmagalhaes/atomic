# Spec de gaps do rendering engine

Levantamento real (código lido, não memória) do estado de `css`, `layout-engine`, `render`, `webgl`, `dom` — 2026-08-23. Cada item vem de leitura direta do source, com `arquivo:linha` quando relevante. Complementa `browser-idle-spec.md` (que cobre a arquitetura geral) focando especificamente no motor de renderização.

Convenção: **REAL** = implementado e testado. **FALTANDO** = confirmado ausente (não parcial, não "quase lá").

## 1. CSS (`crates/css`)

### Seletores (`parser.rs`)
- REAL: tipo (`div`), id (`#foo`), classe (`.foo`), universal (`*`) — `parser.rs:420-452`
- REAL: combinador descendente (espaço) — `parser.rs:405-418`
- FALTANDO: combinador filho `>`
- FALTANDO: combinadores irmão `+`/`~`
- FALTANDO: pseudo-classes (`:hover`, `:first-child`, `:nth-child`, `:not()`, etc.) — lexer não tokeniza nada além do `:` já usado como separador `property:value`
- FALTANDO: pseudo-elementos (`::before`, `::after`)
- FALTANDO: seletores de atributo (`[attr=val]`, `[attr~=val]`, ...) — tokens `[`/`]` nem existem no lexer

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

**Gap total, não parcial.** Zero código em qualquer lugar do workspace:
- Sem dependência de crate de decodificação (`image`, `png`, `jpeg`) em nenhum `Cargo.toml`
- Sem tratamento de `<img>` em `dom`/`html`/`layout-engine`
- Sem `HTMLImageElement`
- Sem textura-a-partir-de-imagem em `render`/`webgl`
- Consequência: nenhuma página com imagem real renderiza a imagem — o `<img>` vira um elemento genérico sem conteúdo visual

## 6. Input real (mouse/teclado → DOM)

**Gap total no nível de interação humana.** O que existe:
- `Profile::click(selector)`/`Profile::fill(selector, value)` — só `#id`, dirigido via protocolo stdin `CLICK`/`FILL` do `profile-worker`, usado por scripts de automação (`pane.click(...)`), não pelo usuário clicando na tela
- `CLICK` dispara um evento `"click"` real via `dom::Dom::find_by_id` + `dispatchEvent` já existente
- `FILL` seta `textContent` (não existe `HTMLInputElement.value`, ver seção 8)

O que falta:
- **Hit-testing por coordenada**: nenhum código traduz a posição de um clique (pixel x,y dentro da textura de um pane) pra qual `LayoutBox`/nó DOM está ali. Clicar na textura de um pane em `apps/shell` só seleciona aquele pane (`main.rs:1164-1165`), nunca chega no conteúdo renderizado
- **Foco/tab order**: `dom::NodeData` não tem estado de foco, `tabindex`, `focus()`, `activeElement` — nada disso existe em `dom` ou `js-runtime`
- **Teclado real**: nenhuma tecla digitada pelo usuário chega em um elemento focado — só existe `FILL` via automação

Consequência prática: um humano não consegue clicar num link ou digitar num campo interagindo com os pixels renderizados. Só é possível via automação `pane.click`/`pane.fill` por id.

## 7. Scroll

**Gap total.** `overflow` não é propriedade de `ComputedStyle`. Sem clipping, sem viewport scrollável maior que o pane, sem barra de rolagem, sem `scrollTop`/`scrollIntoView`. Documentado só em comentário (`display_list.rs:6`), sem nenhum código de suporte.

## 8. Formulários / elementos de input

- FALTANDO: tipo de nó dedicado pra `<input>`/`<textarea>`/`<select>` — `dom::NodeData::Element` é genérico (`{ tag, attributes }`), sem distinção por tag
- FALTANDO: `HTMLInputElement.value` (ou qualquer `.value` de elemento de formulário) — confirmado ausente em `dom` e `js-runtime`
- Consequência: mesmo com hit-testing implementado no futuro, digitar em um "campo" não teria onde escrever — precisaria de um tipo de nó novo com estado de valor próprio

## Resumo de prioridade (se fosse continuar o motor)

Ordenado por "o que mais destrava uso real da engine":

1. **Input real (hit-testing + foco + teclado)** — sem isso, ninguém usa o browser como usuário, só como executor de scripts
2. **Elementos de formulário reais (`value`, foco)** — pré-requisito pro item 1 fazer sentido em páginas com login/busca
3. **Imagens** — a maioria das páginas reais tem pelo menos um `<img>`; sem isso qualquer página parece quebrada
4. **Scroll** — páginas mais altas que o viewport hoje simplesmente cortam sem aviso
5. **`position`/floats** — muito layout real depende disso, mas menos crítico que os 4 acima pra "consigo navegar"
6. **Seletores CSS mais completos** (`>`, pseudo-classes) — cosmético/compat, não bloqueia uso básico
7. **Bordas/box-shadow/opacity** — puramente visual, última prioridade
