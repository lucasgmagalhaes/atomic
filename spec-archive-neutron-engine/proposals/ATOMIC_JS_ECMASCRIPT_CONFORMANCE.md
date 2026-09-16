# AtomicJS — Programa de conformidade ECMA-262

**Status:** proposta de mudança de escopo; não inicia integração de produção

**Última revisão:** 2026-09-15

## 1. Objetivo preciso

Este documento transforma a ambição “implementar ECMAScript completo” em um programa
verificável. O alvo é uma implementação conformante de uma **edição imutável e
nomeada** de [ECMA-262](https://tc39.es/ecma262/), validada por uma revisão fixada do
[Test262](https://github.com/tc39/test262).

“Completo” neste documento significa:

- toda a linguagem, sintaxe, semântica e standard library obrigatórias da edição ECMA-262
  escolhida;
- os testes Test262 aplicáveis ao perfil declarado, sem exclusões permanentes para
  comportamento implementado;
- módulo, promises, jobs, async functions, generators, Proxy, Symbol, BigInt, RegExp,
  typed arrays, coleções fracas e demais semânticas ECMA-262;
- uma implementação host mínima, explícita e estável para os hooks que a especificação
  delega ao host.

Não significa automaticamente:

- **ECMA-402 / `Intl`**: especificação separada, com plano e suite próprios;
- DOM, Fetch, timers, Workers, WebCrypto, WebAssembly ou outras APIs Web;
- compatibilidade de browser em geral; isso exige WPT e especificações adicionais;
- extensões não padronizadas de Node.js, V8 ou QuickJS-ng.

O rótulo “ECMAScript completo” só poderá ser usado publicamente depois da aprovação dos
gates deste documento. Antes disso, AtomicJS deve declarar o subconjunto suportado.

## 2. Governança de versão

ECMAScript evolui anualmente. Não existe “100% para sempre” sem uma política de
atualização. Cada ciclo de compatibilidade deve criar um alvo como:

```text
Target: ECMA-262 <edição publicada e data>
Oracle suite: test262 commit <SHA>
Host profile: atomicjs-es<edição>-core-v1
Annex B profile: web-compatible | excluded-with-rationale
```

A alteração de edição é um projeto separado: atualiza a tag do Test262, lista testes
novos/alterados, classifica cada mudança e só então modifica o runtime. A branch `main`
não é a definição mutável do padrão.

O Test262 é a suite oficial de conformidade e possui mais de 50 mil arquivos de teste;
ele cobre grande parte da gramática e dos algoritmos, mas seus próprios mantenedores não
o consideram uma prova matemática de cobertura total. Por isso a aprovação combina
Test262, revisão de capítulos da especificação e testes próprios de integração. [Test262
README](https://github.com/tc39/test262)

## 3. Decisão arquitetural necessária

O programa ECMA-262 é incompatível com manter AtomicJS como uma spike sem GC, exceções
ou dependentes. Antes da Fase 1, uma decisão explícita deve escolher uma das opções:

| Opção | Consequência |
| --- | --- |
| A. AtomicJS torna-se runtime de produção | autoriza integrar gradualmente atrás de feature flag e assumir manutenção de compatibilidade |
| B. AtomicJS permanece laboratório | o plano pode executar fases de infraestrutura e conformidade, mas não substitui QuickJS-ng |
| C. QuickJS-ng continua definitivo | este documento vira referência, sem execução de implementação ampla |

Nenhuma fase presume a opção A. JIT é independente desta decisão: conformidade vem
primeiro e um JIT nunca é requisito para executar Test262.

## 4. Arquitetura de implementação exigida

O layout atual de uma crate única é adequado para o experimento, mas a conformidade
completa exige fronteiras auditáveis. A migração deve ser incremental, preservando testes
em cada corte:

```text
atomicjs-core       Value, handles, atoms, erros e configuração
atomicjs-parser     lexer, parser, AST, source spans e early errors
atomicjs-bytecode   lowering, verifier, constantes, source maps e exception tables
atomicjs-runtime    realms, intrinsics, globals, jobs e host hooks
atomicjs-object     objetos, shapes, descriptors, prototypes e property keys
atomicjs-gc         heap, roots, tracing, finalização e write barriers
atomicjs-vm         frames, calls, unwinding, interpreter e feedback
atomicjs-builtins   Object/Array/String/Number/... e shared abstract operations
atomicjs-test       runner Test262, metadata, baseline e relatórios
atomicjs-jit        opcional; só após o Tier 0 conformante
```

Não é obrigatório criar todas as crates no início. É obrigatório manter esses papéis
separáveis, evitar ciclos entre parser/runtime/GC e impedir que `unsafe` de código nativo
vaze para a semântica de objetos.

## 5. Perfil de host mínimo

ECMA-262 define hooks ao host. AtomicJS deve documentar e testar um perfil mínimo antes
de executar Test262 em escala:

- criação/destruição de realm e global object;
- `HostEnsureCanCompileStrings` para `eval` e `Function`;
- `HostPromiseRejectionTracker` e fila de jobs/microtasks;
- resolução, carregamento, linking e avaliação de módulos;
- `HostImportModuleDynamically` e `import.meta`, quando módulos forem habilitados;
- timezone, locale e clock apenas onde ECMA-262 exige (sem prometer `Intl`);
- hooks de finalização e integração com GC;
- política de segurança para código dinâmico e carregamento de módulos.

Todo hook deve ter: API Rust, default determinístico para teste, caso de erro e métrica
de chamada. Não esconder semântica da linguagem em um callback de host sem teste.

## 6. Fases de conformidade

As fases são ordenadas por dependências semânticas, não por facilidade de parser. Uma
fase pode ter subfases; não pode declarar concluída apenas por “compilar exemplos”.

### Fase 0 — harness, matriz e baseline

Entregas:

- importar ou fixar Test262 por SHA, sem copiar seus testes para a árvore principal;
- criar runner que entende metadata, includes, negative tests, flags, módulos e async;
- produzir relatório `pass/fail/skip/unsupported` por diretório e por feature;
- manter allowlist temporária com proprietário, causa, issue/decisão e data de expiração;
- executar um subconjunto rápido por PR e matriz completa agendada;
- publicar baseline inicial, inclusive falhas esperadas do subconjunto atual.

Gate: runner reproduz resultados, distingue falha do runtime de falha do harness e não
classifica `skip` como `pass`.

### Fase 1 — sintaxe, early errors e execução básica

Escopo:

- Unicode identifiers, comments, literals e automatic semicolon insertion;
- expressões, statements, bindings, scopes léxico/var, strict mode e eval direto/indireto;
- funções, parâmetros, `this`, classes de ambiente e global environment;
- early errors e mensagens/categorias de `SyntaxError` compatíveis no comportamento.

Infraestrutura:

- source spans, tabela de símbolos e árvore de scopes;
- bytecode verifier com limites de stack, saltos e exception-region;
- parser em modo script e module.

Gate: Test262 dos grupos de lexical grammar, expressions, statements e strict mode
aplicáveis passa; todo parse inválido conhecido retorna erro controlado, sem panic.

### Fase 2 — modelo de valores e conversões abstratas

Escopo:

- `undefined`, `null`, boolean, Number, String, Symbol, BigInt e Object;
- `ToPrimitive`, `ToNumber`, `ToNumeric`, `ToString`, `ToPropertyKey`, igualdade abstrata
  e estrita, relações, truthiness e operadores;
- NaN, `-0`, infinidades, conversões numéricas e formatação especificada;
- property keys string/symbol e ordem de chaves definida.

Gate: cada abstract operation recebe testes direcionados e differential tests; nenhum
fast path pode substituir conversão observável por uma suposição de número.

### Fase 3 — objetos, propriedades e protótipos

Escopo:

- ordinary objects, property descriptors, extensibilidade, selagem/congelamento;
- `[[Get]]`, `[[Set]]`, `[[DefineOwnProperty]]`, `[[Delete]]`, `[[OwnPropertyKeys]]`;
- prototype chain, accessors, receivers, `super` e `this` em métodos;
- Object, Reflect e Proxy, inclusive invariantes de trap;
- Symbol e well-known symbols.

Infraestrutura:

- `PropertyDescriptor` completo, shapes separados de valores e versionamento;
- GC roots de prototypes, descriptors, closures e caches;
- invalidação de inline cache por mudança de descriptor/prototype/Proxy.

Gate: nenhuma otimização de shape/slot é habilitada até os testes de descriptor, accessor,
prototype e Proxy correspondentes passarem em Tier 0.

### Fase 4 — funções, classes e controle não local

Escopo:

- function declarations/expressions, arrows, methods, constructors e `new`;
- bound functions, `call`/`apply`/`bind`, rest/default/destructuring parameters;
- classes, campos públicos/privados, static blocks, inheritance e `super`;
- `throw`, `try`/`catch`/`finally`, Error objects e stack/unwinding;
- generators, iterators, `yield`, `yield*` e async generators.

Infraestrutura:

- exception tables, handler stack, completion records e frame materializável;
- generator/async frame suspensível e rootada pelo GC;
- deotimização de qualquer tier para um PC que preserve exceção pendente.

Gate: `finally` em retorno/throw/break/continue, reentrância de getters e erros durante
unwinding possuem regressões próprias. JIT permanece proibido até esse gate.

### Fase 5 — coleções, arrays, strings, regex e binários

Escopo:

- Array exótico, sparse arrays, length, iteradores, métodos e species;
- String/RegExp, Unicode, captures, flags e métodos de String/RegExp;
- Map, Set, WeakMap, WeakSet e regras de observabilidade/GC de coleções fracas;
- ArrayBuffer, SharedArrayBuffer quando o host oferecer suporte, DataView e typed arrays;
- Atomics e modelo de memória somente com threading host explicitamente seguro.

Infraestrutura:

- representação densa/esparsa de arrays, storage de bytes, allocation accounting;
- motor RegExp validado ou backend cujo contrato seja especificado;
- weak references e finalização integradas ao coletor, não simuladas com `Rc`.

Gate: Test262 de cada família passa. `SharedArrayBuffer`/Atomics podem ser marcados como
perfil host separado, mas não como “implementados” sem garantia de memória correta.

### Fase 6 — promises, jobs, async e modules

Escopo:

- Promise resolution procedure, thenables, rejections e combinators;
- job queue e ordem observável de microtasks;
- `async`/`await`, async iterators e async generators;
- módulos estáticos, live bindings, cyclic graphs, namespace objects e top-level await;
- dynamic `import()` e `import.meta` por hooks de host definidos.

Gate: runner Test262 suporta suites async/módulo; testes de ordem de jobs e ciclos de
módulos passam em host determinístico.

### Fase 7 — standard library completa e Annex B

Escopo:

- todos os construtores, métodos e propriedades obrigatórios ECMA-262;
- Date, JSON, Math, global functions e propriedades de intrinsics;
- `eval`, `Function`/`AsyncFunction` quando permitido pelo host;
- Annex B em perfil web-compatível, com decisão explícita sobre cada item normativo
  opcional e teste correspondente.

Gate: o relatório mostra cobertura por built-in e não deixa “stubs silenciosos”. Uma API
não habilitada pelo perfil deve produzir comportamento especificado/erro explícito, não
valor inventado.

### Fase 8 — conformidade sustentada e atualização anual

Entregas:

- execução integral de Test262 fixado em CI reprodutível;
- zero falhas não classificadas no perfil declarado;
- auditoria manual dos capítulos e dos normative optional features;
- relatório público de versão, host profile, exclusões e limitações;
- processo anual de atualização de ECMA-262/Test262.

Gate: aprovação de arquitetura e release só após relatório assinado por mantenedor de
runtime e responsável pelo harness. “Passa hoje na minha máquina” não é gate.

## 7. Testes e métricas de progresso

O único indicador honesto não é porcentagem bruta de arquivos. O dashboard deve conter:

| Métrica | Uso |
| --- | --- |
| Test262 pass/fail/skip por capítulo/feature | estado de conformidade |
| falhas novas versus baseline | regressão de PR |
| allowlist com expiração | dívida explícita |
| differential com QuickJS-ng | diagnóstico rápido para subconjuntos compartilhados |
| testes metamórficos/fuzzing com seed | bugs entre combinações de semântica |
| cobertura de bytecode/abstract operations | lacunas internas |
| tempo, RSS, alocações e pausas de GC | custo de runtime |
| tempo de startup e de harness | viabilidade operacional |

O runner deve executar testes em isolamento por realm/processo quando a metadata exigir,
evitando estado global contaminado. Falhas precisam incluir fonte, metadata, seed, versão
da suite, bytecode/disassembly opcional e categoria de erro.

## 8. Performance e JIT após conformidade

A ordem obrigatória é:

```text
Tier 0 correto -> Test262 estável -> perfil de produto -> Tier 1 guardado
-> deopt/safepoints corretos -> baseline JIT opcional -> otimizações adicionais
```

Fast paths só podem ser adicionados a uma operação depois de sua versão genérica passar
na suite relevante. Todo guard miss, invalidação de shape, Proxy, accessor, exceção,
`eval`, mudança de prototype e troca de função deve retornar ao caminho correto.

O baseline JIT requer, no mínimo:

- mapas de stack e roots para GC;
- safepoints e reconstrução de frame/PC;
- exception/deopt tables;
- hard cap de código por processo e evicção em pausa;
- feature flag desabilitada por padrão;
- benchmark que inclua compile time, warmup, RSS e p95, não apenas throughput.

Nenhum estágio de JIT reduz o gate de Test262: Tier 0, Tier 1 e código nativo precisam
produzir o mesmo resultado observável para os testes aplicáveis.

## 9. ECMA-402, Web APIs e compatibilidade de browser

Essas áreas são roadmaps irmãos, não tarefas implícitas deste documento:

- **ECMA-402 / Intl:** locale data, ICU, timezone e collation possuem custo substancial;
  requerem Test262 ECMA-402 e orçamento de binário/memória próprios.
- **Web APIs:** DOM, Fetch, timers, streams, workers e WebCrypto dependem de WHATWG/W3C,
  não da ECMA-262; devem usar WPT e testes de integração de host.
- **Node.js:** CommonJS, `process`, filesystem e package resolution são extensão de host.
- **WebAssembly:** runtime separado, não parte de ECMAScript.

Uma release só pode usar a frase “ECMAScript conformante” para ECMA-262 e a edição
específica aprovada. `Intl` ou browser compatibility exigem afirmações separadas.

## 10. Riscos e regras de parada

| Risco | Mitigação/decisão |
| --- | --- |
| escopo multi-ano | marcos por capítulo, sem prometer data até haver equipe/capacidade |
| Test262 runner incorreto | testar o runner contra hosts conhecidos e metadata canônica |
| GC inseguro ou incompleto | construir roots e testes de retenção antes de weak/finalization |
| JIT mascara bug semântico | JIT posterior, feature flag e differential Tier 0/Tier N |
| custo de manutenção excede ganho | reavaliar em cada fase; QuickJS-ng continua alternativa válida |
| mistura com Web APIs | manter suites, ownership e claims separados |

Uma fase deve parar e retornar à decisão arquitetural se seu custo de compatibilidade
eliminar o ganho de cold start/RSS que motivaria AtomicJS, ou se o produto não precisar
mais de um runtime próprio.

## 11. Próxima sequência executável

1. Escolher a opção arquitetural A/B/C e fixar uma edição de ECMA-262 + SHA Test262.
2. Implementar somente a Fase 0: runner, baseline, relatórios e allowlist expirável.
3. Publicar a matriz inicial de lacunas por capítulo.
4. Quebrar a Fase 1 em propostas pequenas, cada uma com subset, testes e benchmark de
   regressão.
5. Reavaliar a opção arquitetural após Fase 3, quando o custo de objetos/GC/prototypes
   estiver mensurável.

Não iniciar por JIT, built-ins avulsos ou uma corrida para aumentar a porcentagem de
arquivos Test262 sem semântica e harness confiáveis.

---

Relacionado: [roadmap técnico condicional](ATOMIC_JS_ROADMAP.md), [validation
spike](ATOMIC_JS_SPIKE.md), [notas de tiering](ATOMIC_JS_TIERING.md) e [índice de
propostas](ATOMIC_JS_INDEX.md).
