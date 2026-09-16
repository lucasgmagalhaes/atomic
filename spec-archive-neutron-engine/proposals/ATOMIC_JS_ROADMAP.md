# AtomicJS — Roadmap técnico condicional

**Status:** planejamento de referência, não autorização de integração

**Última revisão:** 2026-09-15

## 1. Decisão de produto e regra de avanço

AtomicJS continua sendo um experimento isolado em `crates/atomicjs`. O runtime de
produção permanece o QuickJS-ng. Este roadmap descreve a sequência correta caso o
experimento continue produzindo evidência útil; ele não autoriza substituir o runtime
de produção, adicionar dependências a outros crates, nem iniciar um JIT por inércia.

O objetivo possível é um motor especializado para scripts controlados e workloads de
perfil ocioso: baixo custo de partida e memória, boa latência em pequenos bursts e
execução eficiente de loops aritméticos e acessos a objetos simples. Não é competir com
V8, JavaScriptCore ou QuickJS-ng em compatibilidade geral ou throughput universal.

O caminho alternativo para conformidade integral com uma edição nomeada de ECMA-262 está
em [ATOMIC_JS_ECMASCRIPT_CONFORMANCE.md](ATOMIC_JS_ECMASCRIPT_CONFORMANCE.md). Ele é uma
mudança de escopo, com Test262 como gate, e não altera os limites deste roadmap por si só.

Cada etapa só avança quando cumpre, na ordem:

1. semântica definida e testes de regressão;
2. teste diferencial contra QuickJS-ng para o subconjunto suportado;
3. benchmark reproduzível que mede a hipótese da etapa;
4. orçamento de memória e latência sem regressão material;
5. decisão explícita de manter, alterar ou remover a otimização.

Uma otimização que não supere esses gates fica desligada ou é removida. O interpretador
Tier 0 é sempre a autoridade semântica e a rota de fallback.

## 2. Ponto de partida

| Área | Situação atual |
| --- | --- |
| Pipeline | lexer, parser, AST, compilador de bytecode e VM próprios |
| Linguagem | números, booleanos, `undefined`, objetos simples, funções, closures, `if`/`else`, `for`/`while`, chamadas diretas/dinâmicas e `Math.sqrt`/`Math.log` no subconjunto atual |
| Correção | testes unitários, casos gerados e diferencial com QuickJS-ng |
| Tiering | feedback por chamada/loop, orçamento de código, pausa invalida a promoção |
| Tier 1 | executor especializado com operações numéricas, chamadas diretas/inlining estreito e leituras de propriedades numéricas protegidas por identidade, shape e slot |
| Medição | Criterion interno, comparação de funções quentes com QuickJS-ng e executáveis de processo novo |
| Limites | sem exceções, arrays, strings completas, módulos, GC, APIs web ou integração de produção |

O ganho recente em loop de propriedade é uma validação local do Tier 1, não uma prova
de vantagem global: no ambiente de desenvolvimento, o cenário especializado mediu
aproximadamente 6,9 ms contra 12,7 ms no Tier 0. A comparação deve ser repetida em
máquina dedicada antes de orientar uma decisão de produto.

## 3. Arquitetura-alvo por camadas

```text
Fonte JS
  -> parser + AST + diagnósticos
  -> bytecode verificado + metadados de origem
  -> Tier 0: interpretador correto e instrumentado
  -> Tier 1: bytecode especializado e guardado
  -> [gate de produto] compilador baseline para código nativo
  -> [gate de produto] otimizações seletivas / OSR

Todos os tiers -- guard miss, invalidação, pausa ou orçamento --> Tier 0
```

As fronteiras propostas são lógicas, não exigem dividir prematuramente a crate. Uma
separação física só deve ocorrer quando reduzir acoplamento, tempo de compilação ou
risco de teste. O limite atual de módulos de implementação abaixo de 350 linhas e a
API pública pequena devem ser preservados enquanto a crate for experimental.

## 4. Estágios de performance

### Estágio P0 — linha de base confiável

Objetivo: tornar toda decisão mensurável antes de adicionar semântica complexa.

- Fixar um manifesto de workloads: parse, compile, `run_compiled`, cold process,
  memória de pico, memória ociosa, aritmética, objetos, chamadas, closures e falhas de
  guard.
- Versionar ambiente, CPU, compilador Rust, modo de energia, número de amostras e
  comandos usados para cada resultado publicado.
- Rodar Criterion para regressões locais, `hyperfine` ou equivalente para processo
  novo e uma máquina dedicada para números comparativos.
- Separar custo de parse/compilação, execução Tier 0, warmup, execução Tier 1 e memória
  de metadados. Nunca somar esses custos e chamar o resultado de “JIT speed”.
- Registrar percentis e variância; resultados que variam mais que o ganho alegado não
  são decisões de otimização.

Saída: dashboard ou artefato versionado com baseline por commit e limiares de regressão
aprovados. Sem essa saída, nenhum novo tier deve ser iniciado.

### Estágio P1 — Tier 0 rápido e previsível

Objetivo: extrair o máximo do interpretador antes de pagar por compilação adicional.

Prioridade de trabalho:

1. verificador de bytecode, limites de pilha/locais e diagnósticos de bytecode inválido;
2. cache de constantes, dispatch mensurável e reutilização de frames/operand stack;
3. slots de locais tipados quando comprovadamente seguros;
4. shapes transitórios, lookup por slot e inline caches monomórficos no Tier 0;
5. inline caches polimórficos pequenos somente depois de perfis mostrarem múltiplos
   shapes no mesmo site;
6. tabelas de chamadas diretas e metadados de aridade;
7. custo explícito de contadores de feedback e amostragem adaptativa.

Não fazer: threaded dispatch, assembly manual ou mudança de representação de valor sem
perfil que prove que o dispatch/boxing é o gargalo.

Saída: cada otimização mantém o diferencial e reduz uma métrica isolada; nenhuma pode
prejudicar cold start ou RSS acima do orçamento da etapa P0.

### Estágio P2 — Tier 1 especializado

Objetivo: ampliar o executor atual, ainda em bytecode/Rust, para padrões quentes que
possam ser guardados e abandonados sem reconstrução de estado observável.

Sequência sugerida:

1. consolidar guards de número, booleano, objeto, identidade, shape, slot e aridade;
2. adicionar leitura de upvalue protegida e chamadas diretas com argumentos mistos
   apenas se o estado de fallback continuar íntegro;
3. suportar sites de propriedade monomórficos e, depois, polimórficos com limite fixo;
4. especializar operações de comparação, branches e laços sem alocar valores temporários;
5. inlining de folhas pequenas guiado por custo de código e profundidade;
6. perfis por site de chamada/propriedade, não apenas por função;
7. descarte de código especializado na pausa e sob pressão de orçamento.

Regra de segurança: uma função só entra no Tier 1 quando todos os efeitos antes de um
possível guard miss forem reexecutáveis pelo Tier 0 ou tiverem um ponto de saída válido.
Sites que possam chamar código arbitrário, escrever objeto observável ou disparar erro
ficam fora até existir infraestrutura de deotimização.

Saída: matriz explícita “opcode × tipos aceitos × guard × fallback”, taxa de fallback
telemetrizada e benchmarks Tier 0/Tier 1 para cada nova família de instrução.

### Estágio P3 — preparação para código nativo

Objetivo: não construir um JIT antes que sua entrada, saída e invalidação sejam
verificáveis no bytecode.

- Definir um IR linear de baixo nível ou uma forma SSA limitada, com tipos observados e
  localizações de valores por program counter.
- Criar mapas de stack, mapas de safepoint e uma representação de frame que possa ser
  materializada de volta no Tier 0.
- Projetar deopt antes de gerar a primeira instrução nativa: guard miss deve preservar
  valor de operand stack, locais, PC, exceção pendente e cadeia de chamadas.
- Separar o cache de código, metadados de profiling e memória executável, com contagem
  exata para o orçamento por processo.
- Implementar invalidação de inline cache e de código dependente de shape/versão de
  função, inicialmente de modo conservador.
- Adicionar modo determinístico que force todos os guards a falhar e compare o resultado
  com Tier 0; isso testa a saída antes de testar velocidade.

Gate obrigatório: um perfil real de produto deve mostrar que o tempo de JS em execução
ativa domina uma latência perceptível, e a estimativa de RSS do código nativo precisa
ficar dentro do orçamento P0. Sem os dois, P3 é o fim da linha.

### Estágio P4 — baseline JIT experimental

Objetivo: testar, atrás de feature flag, se código nativo simples paga seu custo.

Escopo inicial:

- apenas funções numéricas monomórficas sem alocação, exceção, chamada dinâmica ou
  captura mutável;
- compilação síncrona controlada em benchmark e assíncrona somente após safepoints e
  instalação atômica estarem prontos;
- backend preferencial de manutenção baixa (por exemplo Cranelift), sem assembler
  próprio;
- guard de entrada, guard nos sites de propriedade e deopt no primeiro miss;
- hard cap de memória por processo, feature flag desligada por padrão e evicção ao pausar.

Métricas: custo de compilação, tamanho de código, taxa de deopt, tempo até break-even,
p95 de latência de chamada e RSS depois de pausar. A pergunta é “o usuário recupera o
custo enquanto a página está ativa?”, não “qual o maior número de ops/s?”.

Saída: manter somente se houver ganho sustentado depois de incluir warmup e memória. Se
o break-even exceder a vida típica de um script ativo, remover ou manter apenas para
benchmarks, sem integração de produto.

### Estágio P5 — JIT de segunda camada, somente com evidência forte

Objetivo: otimizar os poucos casos que sobreviverem ao baseline JIT em carga real.

- coleta de type feedback por site, perfis de branch e contagem de alocações;
- propagação de tipos, eliminação de checagens redundantes e folding de constantes;
- scalar replacement e escape analysis apenas para objetos de vida local comprovada;
- inlining com orçamento de código e política de reversão;
- OSR para entrar/sair de loops longos, usando os mapas definidos em P3;
- compilação concorrente com cancelamento em pausa;
- evicção por LRU/benefício real se o hard cap não for suficiente.

Não fazer nesta etapa sem nova decisão: JIT otimizador geral, speculation agressiva,
geração de SIMD, WebAssembly, suporte a múltiplas arquiteturas não usadas pelo produto
ou um coletor que exista apenas para alimentar benchmarks.

## 5. Roadmap de linguagem e runtime

O avanço de linguagem deve ser orientado por programas-alvo, não pela ordem do padrão
ECMAScript. Cada item precisa de especificação do subconjunto, diagnóstico para forma
não suportada e caso diferencial.

| Onda | Capacidades | Dependências e gates |
| --- | --- | --- |
| L0 — consolidar | precedência, coerções já aceitas, escopo, closures, recursão, `if`/laços e objetos simples | verificador de bytecode e diferencial ampliado |
| L1 — valores úteis | `null`, strings, igualdade/relacionais completos, operadores lógicos, `typeof`, arrays densos pequenos | modelo de `Value`, semântica de conversão e testes de borda |
| L2 — objetos reais | chaves dinâmicas, delete, enumeração definida, protótipo deliberadamente limitado, métodos e `this` | shapes/versionamento, invalidadores de IC |
| L3 — controle e erros | `throw`, `try`/`catch`/`finally`, erros estruturados, stack trace e tabelas de exceção | unwinding no Tier 0; JIT bloqueado até deopt com exceção funcionar |
| L4 — funções modernas | rest/spread selecionados, arrow functions, defaults, destructuring restrito, classes somente se workloads exigirem | alocação, chamadas e diagnósticos maduros |
| L5 — módulos e async | módulos estáticos, promises/microtasks, `async`/`await`, cancelamento host | scheduler e integração de host; não pertencem ao experimento inicial |

Strings, arrays, exceções, protótipos e async alteram fundamentalmente alocação e
observabilidade. Eles não devem ser “pequenas features” em commits de performance.

## 6. Memória, alocação e GC

Enquanto AtomicJS processar scripts pequenos e de vida curta, medir um arena/bump
allocator ou uma estratégia de ciclo de vida explícita pode ser suficiente. GC só entra
quando houver dados de objetos de vida longa, ciclos ou pressão de memória que não possa
ser resolvida pelo ciclo de vida do programa.

Ordem segura:

1. contabilidade de bytes por tipo de `Value`, objeto, constante, frame, feedback e código;
2. testes de vazamento/retenção com workloads repetidos;
3. alocador de objetos separado de metadados imutáveis;
4. roots explícitas: stack, locals capturados, globais, cache de código e handles do host;
5. mark-and-sweep stop-the-world simples, se e somente se necessário;
6. write barrier e geração jovem apenas após perfil demonstrar que coleta simples é gargalo.

GC e JIT se encontram nos safepoints e mapas de stack. Não implementar código nativo
que esconda roots de um coletor futuro; não implementar GC geracional antes de haver
pressão de alocação medida.

## 7. Correção, segurança e qualidade

- Manter QuickJS-ng apenas como oráculo de desenvolvimento, nunca dependência do caminho
  de produção do AtomicJS.
- Ampliar geração determinística de programas por famílias de sintaxe, com seed e shrink
  reproduzíveis.
- Adicionar testes metamórficos: interpretar uma vez/muitas vezes, trocar layout de objeto,
  forçar miss de guard, pausar/retomar e invalidar chamadas diretas.
- Quando exceções existirem, comparar valor **ou** categoria/mensagem de erro, não apenas
  texto de conclusão.
- Incluir verificador de bytecode, limites configuráveis de recursão/stack e proteção
  contra loops de compilação/deopt.
- Rodar Miri ou sanitizers onde aplicável; isolar todo `unsafe` de backend JIT em crate ou
  módulo pequeno, auditável e sem acesso direto a objetos de alto nível.
- Preservar serialização/disassembly de bytecode para reproduzir falhas fora do parser.

## 8. Observabilidade necessária

O runtime deve expor uma estrutura de diagnóstico opt-in, sem alocar no caminho frio:

- contagens por função e por site: chamadas, backedges, shapes vistos, tipos vistos;
- promoções, instalações, bytes de código, evicções por pausa/orçamento;
- acertos/misses de IC e causas de fallback/deopt;
- tempo de parse, compilação, Tier 0, Tier 1, compilação nativa e GC;
- profundidade máxima de frame/stack e bytes vivos por categoria;
- versão de runtime, feature flags e fingerprint do benchmark.

Relatórios agregados devem ser coletados apenas em desenvolvimento ou amostragem
controlada. Em produção futura, telemetry não pode anular a economia de memória que o
projeto tenta provar.

## 9. Backlog priorizado

### Próximos itens (sem JIT)

1. Formalizar a matriz de suporte de bytecode e executar um bytecode verifier.
2. Criar manifesto de benchmark e armazenar baselines de máquina dedicada.
3. Medir alocação e RSS por fase, não apenas tempo de CPU.
4. Expandir o diferencial para guards de propriedade, trocas de shape, chamadas e pausa.
5. Evoluir o Tier 1 por famílias pequenas: propriedades/upvalues, branches e chamadas,
   cada uma com fallback testado.
6. Adicionar strings/arrays **somente** quando houver workload-alvo e orçamento de
   semântica aprovado.

### Itens bloqueados por decisão ou evidência

- Tornar AtomicJS dependência de `js-runtime`/`profile-worker`.
- GC de produção.
- JIT baseline.
- Exceções, módulos e async.
- JIT otimizador e OSR.

## 10. Critérios de decisão final

Há três resultados aceitáveis:

| Resultado | Evidência | Ação |
| --- | --- | --- |
| Continuar como laboratório | ganhos locais e boa ferramenta de pesquisa, sem ganho de produto comprovado | manter isolado, limitar escopo e usar para aprender |
| Promover um subconjunto | benefício consistente de RSS/cold start/latência em workload de perfil real, sem perda de compatibilidade necessária | abrir uma decisão de arquitetura separada e planejar integração gradual atrás de flag |
| Encerrar ou congelar | QuickJS-ng vence no custo total, o subconjunto requerido cresce demais ou o JIT não recupera warmup/memória | preservar benchmarks e resultados, parar expansão e não integrar |

Nenhuma dessas conclusões deve ser tomada por um microbenchmark isolado. A decisão de
produção precisa comparar processo completo, perfis simultâneos, páginas representativas,
tempo de partida, RSS ocioso, p95 de interação e custo de manutenção.

## 11. Definition of done por etapa

Uma etapa do roadmap está pronta somente quando possui:

- documento curto de escopo e não-objetivos;
- testes unitários e diferencial para o novo subconjunto;
- teste de fallback/deopt, quando houver specialization;
- benchmark Tier 0/Tier N e custo de warmup;
- medição de memória se a etapa cria cache, metadados ou código;
- resultado reproduzível e decisão escrita; e
- rollback simples por feature flag ou remoção do tier, sem alterar a semântica do Tier 0.

---

Leitura relacionada: [validation spike](ATOMIC_JS_SPIKE.md), [notas de tiering]
(ATOMIC_JS_TIERING.md), [protocolo de memória](ATOMIC_JS_MEMORY_MEASUREMENT.md) e
[índice de propostas](ATOMIC_JS_INDEX.md).
