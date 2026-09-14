# ATOMIC_JS_ARCHITECTURE.md

> **Status: REJECTED (2026-09-14).** This proposes replacing the JS engine with a
> from-scratch VM/GC/JIT ("AtomicJS"). That contradicts an already-recorded decision:
> [`mockup/browser-idle-spec.md`](../../mockup/browser-idle-spec.md)'s crate table names
> QuickJS-ng as the JS engine, and [`spec/ROADMAP.md`](../ROADMAP.md)'s
> "Product scope decision (2026-08-26)" names QuickJS-ng's no-JIT/small-heap profile as
> the product's actual competitive edge — explicitly warning against chasing raw JS
> throughput. `crates/js-runtime` already has ~36k LOC / 179 commits and is 75% through
> the ECMAScript matrix on top of QuickJS-ng (see `spec/INDEX.md`). Kept here, unlinked
> from `spec/INDEX.md`'s reading order, as a record of the idea and its rationale —
> not as an active plan. See chat history (2026-09-14) for the full review.
>
> A scoped, time-boxed validation spike replaces this proposal — see
> [`ATOMIC_JS_SPIKE.md`](ATOMIC_JS_SPIKE.md). It does not commit to building a custom
> engine; it tests the narrow part of this thesis worth knowing the answer to, cheaply,
> before any real commitment.

## 0. Objetivo

AtomicJS é o runtime JavaScript próprio do Atomic Browser.

O objetivo **não** é reproduzir V8. O objetivo é construir um engine compacto, rápido no startup, eficiente em memória e capaz de escalar de interpretação para JIT apenas quando isso gerar benefício real.

### Prioridades

1. Startup baixo.
2. Baixo uso de memória.
3. Baixa latência de execução.
4. Boa performance em código interativo e workloads de idle games.
5. Arquitetura incremental.
6. Isolamento forte entre JavaScript e o restante do browser.
7. Cada camada deve poder ser medida e substituída sem reescrever o runtime inteiro.

### Não-objetivos iniciais

- Não implementar um clone de TurboFan.
- Não implementar WebAssembly inicialmente.
- Não implementar todas as otimizações possíveis de JavaScript.
- Não suportar todas as APIs Web antes de o núcleo da VM estar estável.
- Não otimizar microbenchmarks sacrificando startup/memória.
- Não fazer JIT antes de existir um interpreter correto e um profiler confiável.

---

# 1. Modelo mental

A arquitetura base é:

```text
                        JavaScript
                            |
                         Parser
                            |
                          AST
                            |
                      Bytecode Compiler
                            |
                         Bytecode
                            |
                       Interpreter
                            |
                 Profiling / Feedback
                            |
                 +----------+----------+
                 |                     |
              COLD/WARM              HOT
                 |                     |
             Interpreter         Baseline JIT
                                       |
                                  Very Hot
                                       |
                                  Mid-tier JIT
                                       |
                                  Machine Code
```

O runtime deve sempre ter um caminho de fallback:

```text
JIT code
   |
guard/type miss
   v
Runtime / Interpreter
```

O JIT nunca deve ser requisito para a correção da linguagem.

---

# 2. Princípios de arquitetura

## 2.1 Correção antes de performance

A ordem é:

```text
ECMAScript semantics
        >
runtime correctness
        >
memory safety
        >
profiling correctness
        >
performance
```

Nenhuma otimização pode mudar a semântica observável.

## 2.2 Lazy by default

Sempre que possível:

- parsear sob demanda;
- compilar funções sob demanda;
- materializar builtins sob demanda;
- JITar sob demanda;
- alocar estruturas maiores sob demanda;
- carregar APIs Web sob demanda.

## 2.3 Cold code é barato

Código executado poucas vezes não deve pagar:

- custo alto de compilação;
- custo de profiling desnecessário;
- JIT;
- otimizações agressivas.

## 2.4 Hot code recebe investimento

Código quente pode pagar por:

- profiling;
- baseline compilation;
- type feedback;
- IC;
- mid-tier compilation;
- OSR futuramente.

## 2.5 Browser-first

A engine deve ser projetada para:

- execução interativa;
- eventos;
- timers;
- DOM;
- fetch;
- background tabs;
- múltiplos realms;
- workers;
- suspensão e retomada.

Não deve ser projetada apenas para benchmarks de CPU.

---

# 3. Estrutura do workspace Rust

Recomendação inicial:

```text
atomic/
├── Cargo.toml
├── crates/
│   ├── atomic-js-core/
│   ├── atomic-js-parser/
│   ├── atomic-js-bytecode/
│   ├── atomic-js-vm/
│   ├── atomic-js-object/
│   ├── atomic-js-gc/
│   ├── atomic-js-builtins/
│   ├── atomic-js-runtime/
│   ├── atomic-js-profiler/
│   ├── atomic-js-jit/
│   ├── atomic-js-compiler/
│   ├── atomic-js-host/
│   ├── atomic-js-web/
│   └── atomic-js-test/
│
├── benches/
│   ├── startup/
│   ├── parser/
│   ├── interpreter/
│   ├── objects/
│   ├── arrays/
│   ├── properties/
│   ├── calls/
│   ├── loops/
│   ├── gc/
│   ├── jit/
│   └── workloads/
│
├── tests/
│   ├── language/
│   ├── builtins/
│   ├── modules/
│   ├── gc/
│   ├── jit/
│   └── web/
│
└── tools/
    ├── disassembler/
    ├── bytecode-dump/
    ├── heap-inspector/
    └── benchmark-runner/
```

---

# 4. Responsabilidade de cada crate

## atomic-js-core

Tipos fundamentais compartilhados.

Responsabilidades:

- `Value`
- IDs/handles
- `Atom`
- `SymbolId`
- `ObjectId`
- `FunctionId`
- source locations
- runtime errors
- common enums
- configuration

Não deve depender do parser, VM ou JIT.

---

## atomic-js-parser

Responsabilidades:

- lexer;
- tokenizer;
- parser;
- AST;
- source positions;
- parsing de scripts/modules.

Não deve conhecer o browser.

---

## atomic-js-bytecode

Responsabilidades:

- bytecode instruction set;
- bytecode builder;
- bytecode metadata;
- constants;
- local slots;
- exception tables;
- source maps;
- bytecode verifier.

Entrada:

```text
AST
```

Saída:

```text
BytecodeModule
```

---

## atomic-js-vm

Responsabilidades:

- interpreter;
- call frames;
- execution loop;
- exception propagation;
- function calls;
- closures;
- bytecode dispatch;
- interaction com runtime.

Não deve conter implementação específica de DOM.

---

## atomic-js-object

Responsabilidades:

- JS object model;
- prototypes;
- Shapes;
- property storage;
- property descriptors;
- arrays;
- functions;
- exotic objects.

---

## atomic-js-gc

Responsabilidades:

- heap;
- allocations;
- young generation;
- old generation;
- write barriers;
- minor GC;
- major GC;
- incremental GC futuro;
- weak references.

---

## atomic-js-builtins

Responsabilidades:

- `Object`
- `Array`
- `Function`
- `String`
- `Number`
- `Boolean`
- `Math`
- `JSON`
- `Date`
- `Map`
- `Set`
- `Promise`
- etc.

Builtins devem poder ser materializados/lazy-loaded.

---

## atomic-js-runtime

Camada que conecta:

- VM;
- GC;
- objects;
- builtins;
- profiler;
- JIT;
- host APIs.

É o runtime de alto nível.

---

## atomic-js-profiler

Responsabilidades:

- execution counters;
- call counters;
- loop counters;
- type feedback;
- shape feedback;
- property feedback;
- hotness;
- tiering decisions.

O profiler não deve compilar código diretamente.

Ele publica feedback para o `TierManager`.

---

## atomic-js-jit

Responsabilidades:

- baseline JIT;
- futuramente mid-tier JIT;
- code objects;
- executable memory;
- deoptimization metadata;
- OSR metadata.

---

## atomic-js-compiler

Responsabilidades:

- AST -> bytecode;
- bytecode -> IR;
- IR transforms;
- lowering.

Separar compiler de JIT permite usar a mesma pipeline para diferentes backends.

---

## atomic-js-host

Bridge entre o runtime JS e o processo host.

Responsabilidades:

- timers;
- console;
- filesystem quando autorizado;
- network abstractions;
- messaging;
- host objects.

Não expor ponteiros Rust diretamente ao JS.

---

## atomic-js-web

Implementações das APIs Web:

- DOM bindings;
- events;
- fetch;
- URL;
- storage;
- Web APIs.

Esta crate depende do runtime, mas o core JS não deve depender dela.

---

# 5. Dependências entre crates

Regra:

```text
core
  |
  +--> parser
  +--> bytecode
  +--> object
  +--> gc
       |
       v
      vm
       |
 profiler
       |
 compiler
       |
   jit
       |
 runtime
       |
 host/web
```

Evitar ciclos.

Uma direção preferencial:

```text
core
  ↓
low-level runtime pieces
  ↓
execution
  ↓
host integration
```

---

# 6. Value representation

Primeira versão:

```rust
pub enum Value {
    Undefined,
    Null,
    Bool(bool),
    Int32(i32),
    Number(f64),
    Object(GcRef),
    String(StringId),
    Symbol(SymbolId),
    BigInt(BigIntId),
}
```

Porém, isso é apenas a primeira implementação.

Meta de performance:

```text
64-bit compact representation
```

A evolução pode ser:

```text
NaN-boxing / tagged 64-bit
```

ou outro esquema medido como superior.

## Regra

Não otimizar `Value` antes de existirem benchmarks do interpreter.

---

# 7. GC handles

JavaScript nunca deve armazenar ponteiro Rust cru.

Usar IDs/handles:

```rust
pub struct GcRef {
    index: u32,
    generation: u32,
}
```

ou outro handle seguro.

A engine deve invalidar/validar gerações para impedir use-after-free lógico.

---

# 8. Object model

Objeto básico:

```rust
pub struct JsObject {
    shape: ShapeId,
    properties: PropertyStorage,
    prototype: Option<ObjectId>,
    flags: ObjectFlags,
}
```

Shape:

```rust
pub struct Shape {
    id: ShapeId,
    parent: Option<ShapeId>,
    transitions: TransitionTable,
    properties: ShapeProperties,
}
```

Transição:

```text
Shape A
   |
   +-- add x --> Shape B
   |
   +-- add y --> Shape C
```

Objetos com a mesma estrutura podem compartilhar Shape.

---

# 9. Property access

Fluxo inicial:

```text
GET_PROPERTY
     |
     v
shape lookup
     |
     v
property descriptor
     |
     v
offset
     |
     v
load value
```

Depois do IC:

```text
check shape
     |
   match
     |
direct load
```

---

# 10. Inline Cache

Estados:

```text
Uninitialized
    ↓
Monomorphic
    ↓
Polymorphic
    ↓
Megamorphic
```

Estrutura aproximada:

```rust
pub enum InlineCacheState {
    Uninitialized,
    Mono {
        shape: ShapeId,
        offset: u32,
    },
    Poly {
        entries: SmallVec<[IcEntry; 4]>,
    },
    Mega,
}
```

Inicialmente suportar apenas:

- property load;
- property store;
- call.

Depois:

- element access;
- global lookup;
- arithmetic specialization.

---

# 11. Bytecode

Primeira ISA recomendada:

```text
NOP

LOAD_CONST
LOAD_UNDEFINED
LOAD_NULL
LOAD_TRUE
LOAD_FALSE

LOAD_LOCAL
STORE_LOCAL

LOAD_GLOBAL
STORE_GLOBAL

GET_PROPERTY
SET_PROPERTY
GET_ELEMENT
SET_ELEMENT

ADD
SUB
MUL
DIV
MOD

NEG
NOT

EQ
STRICT_EQ
LT
LE
GT
GE

JUMP
JUMP_IF_TRUE
JUMP_IF_FALSE

CALL
CONSTRUCT
RETURN

NEW_OBJECT
NEW_ARRAY

THROW

TRY_BEGIN
TRY_END

POP
DUP
```

Não tentar desenhar 200 opcodes antes de ter workload real.

---

# 12. VM execution model

Preferência inicial:

```text
register-based bytecode
```

Exemplo:

```text
LOAD_ARG    r0, 0
LOAD_ARG    r1, 1
ADD         r2, r0, r1
RETURN      r2
```

Frame:

```rust
pub struct CallFrame {
    function: FunctionId,
    bytecode: BytecodeId,
    pc: usize,
    registers: RegisterFile,
    locals: LocalSlots,
    previous: Option<FrameId>,
}
```

O layout precisa ser planejado já pensando em compatibilidade com o baseline JIT.

---

# 13. Exception handling

Não usar unwind Rust como mecanismo da semântica JavaScript.

JavaScript precisa de:

```text
throw
try
catch
finally
```

A VM deve ter uma estrutura de exception handlers:

```rust
pub struct ExceptionHandler {
    start_pc: u32,
    end_pc: u32,
    handler_pc: u32,
    finally_pc: Option<u32>,
}
```

---

# 14. Functions and closures

Function object:

```rust
pub struct JsFunction {
    code: FunctionCode,
    environment: EnvironmentRef,
    prototype: Option<ObjectId>,
}
```

Environment:

```rust
pub struct Environment {
    parent: Option<EnvironmentRef>,
    bindings: BindingStorage,
}
```

O escape de closures deve ser tratado explicitamente pelo runtime.

---

# 15. Parser strategy

Não manter AST indefinidamente.

Fluxo:

```text
Source
  ↓
Lexer
  ↓
Parser
  ↓
AST
  ↓
Bytecode compiler
  ↓
Bytecode
  ↓
AST dropped
```

Guardar source positions compactas para debugging e stack traces.

---

# 16. Lazy functions

Cada função deve poder permanecer não compilada:

```text
FunctionStub
    |
    +-- source range
    +-- params metadata
    +-- lazy compile state
```

Quando chamada:

```text
FunctionStub
   ↓
compile
   ↓
BytecodeFunction
```

Isso deve valer também para funções aninhadas quando possível.

---

# 17. Modules

Primeira arquitetura:

```text
ModuleRegistry
   |
ModuleRecord
   |
Dependency graph
   |
Evaluation order
```

Fases:

1. resolve;
2. parse;
3. instantiate;
4. evaluate.

Cachear módulos por URL/origin.

---

# 18. Profiler

Profiler por função:

```rust
pub struct FunctionFeedback {
    call_count: u32,
    loop_count: u32,
    execution_ticks: u64,
    property_feedback: FeedbackTable,
    type_feedback: TypeFeedbackTable,
}
```

Não atualizar estruturas caras a cada opcode.

Preferir contadores amostrados ou incrementos muito baratos.

---

# 19. Hotness model

Primeira política:

```text
COLD
< threshold A

WARM
threshold A .. threshold B

BASELINE
threshold B .. threshold C

HOT
> threshold C
```

Mas a decisão final deve considerar:

```text
execution_frequency
×
estimated_compile_cost
×
expected_future_savings
```

Exemplo:

```text
small function + executed 2,000x
=> baseline

large function + executed 50x
=> interpreter

tight loop + executed 100,000x
=> optimize
```

---

# 20. Background compilation

Nunca bloquear a JS thread desnecessariamente.

Arquitetura:

```text
JS thread
   |
   +--> CompilationRequest
            |
            v
       JIT worker pool
            |
            v
       CompiledCode
            |
            v
       install at safepoint
```

O resultado do JIT deve ser instalado somente em pontos seguros.

---

# 21. Baseline JIT

Objetivo:

```text
Bytecode
  ↓
fast translation
  ↓
native code
```

Não executar otimizações complexas.

Primeiros suportes:

- locals;
- constants;
- arithmetic;
- branches;
- calls;
- returns;
- property ICs;
- array indexing.

---

# 22. Backend

Primeira implementação recomendada:

```text
Atomic IR
   ↓
Cranelift
   ↓
native code
```

Não implementar assembler x86/ARM manualmente.

A camada deve ser:

```rust
trait CodegenBackend {
    fn compile(
        &self,
        function: &IrFunction,
        target: TargetInfo,
    ) -> Result<CodeObject, CompileError>;
}
```

Permitir futuro backend próprio.

---

# 23. Baseline compiler representation

O baseline JIT deve preservar informações suficientes para:

- stack walking;
- exceptions;
- profiler;
- debugging;
- deoptimization futura.

`CodeObject`:

```rust
pub struct CodeObject {
    entry: CodePtr,
    size: usize,
    source_map: SourceMap,
    frame_layout: FrameLayout,
    deopt_metadata: Option<DeoptMetadata>,
}
```

---

# 24. Mid-tier JIT

Não implementar antes de o baseline JIT estar medido.

Inspirar-se na filosofia do Maglev:

```text
Bytecode
   ↓
CFG
   ↓
SSA IR
   ↓
small number of optimization passes
   ↓
lowering
   ↓
codegen
```

Passes iniciais:

1. constant folding;
2. dead branch elimination;
3. local value propagation;
4. integer specialization;
5. direct property access using shape guards;
6. simple inlining de funções pequenas.

---

# 25. Type feedback

Registrar:

```text
ADD:
    Int32 + Int32
    Number + Number
    String + String
```

Não assumir tipos sem guard.

IR:

```text
CheckInt32 a
CheckInt32 b
AddInt32
```

Se falhar:

```text
deopt/fallback
```

---

# 26. Deoptimization

Primeira versão simplificada:

```text
optimized code
    |
guard failure
    |
runtime fallback
    |
interpreter
```

A transição pode ocorrer em safepoint.

Guardar materialização suficiente para reconstruir:

```text
locals
arguments
stack
environment
```

A deoptimization sofisticada fica para fase posterior.

---

# 27. OSR

Somente depois da otimização funcionar.

Fluxo:

```text
interpreter
   |
hot loop detected
   |
compile loop
   |
OSR entry
   |
continue from current iteration
```

Prioridade alta para:

- game loops;
- simulation loops;
- rendering logic;
- idle calculations.

---

# 28. Garbage collector

## Fase 1

Single-threaded:

```text
Nursery
  ↓
Minor GC / copying

Old generation
  ↓
Mark-sweep
```

## Fase 2

Adicionar:

```text
write barriers
incremental marking
parallel marking
concurrent sweeping
```

## Fase 3

Medir se compacting GC é necessário.

Não copiar V8 inteira.

---

# 29. Allocation strategy

Preferência:

```text
bump allocation
```

na nursery.

Exemplo:

```text
nursery:
[used][used][free----------------]
             ^
          allocation pointer
```

Objetos pequenos devem ser extremamente baratos de alocar.

---

# 30. Write barriers

Necessário para GC geracional:

```text
old object
    |
    +--> young object
```

registrar essa referência.

Interface:

```rust
fn write_barrier(
    owner: ObjectId,
    value: Value,
);
```

---

# 31. Weak references

Implementar depois do GC básico.

Necessário para:

- `WeakMap`;
- `WeakSet`;
- `WeakRef`;
- `FinalizationRegistry`.

Não deixar essas features bloquearem o runtime inicial.

---

# 32. Strings

Strings precisam de representação específica.

Inicialmente:

```text
FlatString
```

Depois considerar:

```text
String
ConcatString
SliceString
ExternalString
```

Interning:

```text
Atom table
```

para identifiers e strings frequentemente comparadas.

---

# 33. Arrays

Array separado de objeto comum.

Representação inicial:

```text
JsArray {
    elements: ElementsStorage,
    length: u32,
    shape: ShapeId,
}
```

Element kinds:

```text
PackedInt32
PackedNumber
PackedValue
HoleyValue
```

Só expandir se benchmarks reais justificarem.

---

# 34. Builtins

Builtins divididos em:

```text
essential
optional
lazy
```

Essential:

```text
Object
Function
Array
String
Number
Boolean
Math
JSON
```

Optional/lazy:

```text
Intl
Temporal
advanced collections
etc.
```

A lista exata deve seguir a implementação ECMAScript escolhida.

---

# 35. Startup snapshot

Build-time:

```text
initialize builtins
create intrinsic objects
create prototype graph
serialize snapshot
```

Runtime:

```text
load snapshot
map/read immutable data
create mutable heap portions
```

Snapshot deve ter versão:

```text
engine_version
bytecode_version
snapshot_version
target_arch
```

---

# 36. Lazy builtins

Não desserializar/materializar tudo no startup.

Registry:

```rust
enum BuiltinState {
    Lazy(BuiltinId),
    Materialized(ObjectId),
}
```

---

# 37. Bytecode cache

Cache por:

```text
origin
script URL
content hash
engine version
bytecode version
target
security configuration
```

Cache contém:

```text
bytecode
constant pools
metadata
source mappings compactos
```

Não cachear machine code na primeira fase.

---

# 38. HTTP/cache integration

Fluxo:

```text
HTTP cache
   |
   +-- source
   |
   +-- bytecode cache
```

Se hash mudou:

```text
invalidate bytecode
```

Nunca reutilizar bytecode incompatível.

---

# 39. Multiple realms

Modelo:

```text
Engine
 |
 +-- Realm A
 +-- Realm B
 +-- Realm C
```

Compartilhar:

```text
immutable builtins
bytecode handlers
engine metadata
```

Separar:

```text
global object
heap state
DOM bindings
security origin state
```

---

# 40. Web workers

Inicialmente cada Worker deve possuir runtime isolado.

Mais tarde:

```text
shared read-only runtime structures
```

mas nunca compartilhar heap mutável diretamente.

Comunicação:

```text
postMessage
structured clone
transferables
```

---

# 41. JS <-> DOM bridge

JavaScript:

```text
HTMLElement
```

internamente:

```text
HostObject {
    dom_node_id: DomNodeId
}
```

Nunca:

```text
HTMLElement -> raw *mut DomNode
```

Toda operação:

```text
JS object
  ↓
host handle
  ↓
DOM subsystem
```

---

# 42. Security boundaries

Princípio:

```text
untrusted JS
    |
    v
AtomicJS heap
    |
    +----> validated host handles
```

O JavaScript não pode fabricar:

```text
Rust pointers
file descriptors
process handles
memory addresses
```

---

# 43. Scheduling e background tabs

Adicionar `ExecutionPolicy`:

```rust
enum ExecutionPolicy {
    Foreground,
    VisibleBackground,
    Background,
    Frozen,
}
```

E adaptar:

- timer budgets;
- JIT thresholds;
- GC aggressiveness;
- compilation workers;
- event processing.

Exemplo:

```text
Foreground
    => aggressive optimization

Background
    => high JIT threshold

Frozen
    => no JS execution
```

---

# 44. Idle-game specialization sem quebrar generalidade

Não criar APIs JavaScript especiais para idle games.

Em vez disso, otimizar padrões gerais:

```text
tight loops
number arithmetic
property access
array iteration
small function calls
timers
```

Esses padrões naturalmente beneficiam jogos.

---

# 45. Interpreter dispatch

Primeira versão pode usar:

```rust
loop {
    let instruction = bytecode.fetch(pc);
    dispatch(instruction);
}
```

Depois medir:

- match dispatch;
- threaded dispatch quando portátil;
- superinstructions.

Superinstructions candidatas:

```text
LOAD_LOCAL + LOAD_LOCAL + ADD
LOAD_LOCAL + LOAD_CONST + ADD
GET_PROPERTY + RETURN
```

Só adicionar após profiling.

---

# 46. GC e JIT interaction

O JIT deve informar ao GC:

- referências em registers;
- stack slots;
- spill slots;
- embedded references;
- code object metadata.

Cada `CodeObject` precisa de:

```text
safepoint metadata
```

para encontrar roots.

---

# 47. Executable memory

JIT precisa respeitar W^X:

```text
Writable XOR Executable
```

Fluxo:

```text
allocate RW
   ↓
emit code
   ↓
change permissions
   ↓
RX
```

Não manter páginas RWX.

---

# 48. Threading model

Threads principais:

```text
Browser UI
    |
Renderer
    |
JS worker(s)
    |
+---+--------------------+
|                        |
JIT workers          GC workers
```

A primeira versão pode rodar tudo de forma mais simples.

A infraestrutura de filas deve ser desenhada cedo para permitir paralelismo futuro.

---

# 49. Observability

Adicionar desde o começo:

```text
--trace-js
--trace-gc
--trace-jit
--trace-tiering
--dump-bytecode
--dump-shapes
--dump-ic
--dump-heap
```

Métricas:

```text
parse_us
bytecode_compile_us
execute_us
jit_compile_us
jit_code_bytes
gc_pause_us
heap_bytes
native_code_bytes
```

---

# 50. Benchmark runner

Executável:

```text
atomic-js-bench
```

Exemplo:

```bash
atomic-js-bench ./bench/loops.js
atomic-js-bench --engine v8
atomic-js-bench --engine quickjs
atomic-js-bench --engine atomic
```

Output:

```text
Benchmark: numeric_loop

AtomicJS interpreter:  12.4 ms
AtomicJS baseline:      4.1 ms
QuickJS:                6.7 ms
V8 jitless:             8.2 ms
V8:                     2.1 ms
```

Não otimizar sem números.

---

# 51. Test strategy

## ECMAScript correctness

Usar testes de conformidade e uma suíte incremental.

Categorias:

```text
syntax
values
operators
objects
prototypes
functions
closures
classes
exceptions
modules
async
promises
collections
symbols
bigints
```

## Differential testing

Executar o mesmo script em:

```text
AtomicJS
QuickJS
V8
JavaScriptCore
```

e comparar:

```text
output
exceptions
observable behavior
```

Isso é uma das ferramentas mais importantes para o desenvolvimento.

---

# 52. Fuzzing

Fuzzing obrigatório para:

- parser;
- bytecode compiler;
- interpreter;
- GC;
- JIT guards;
- deopt;
- property lookup.

Fluxo:

```text
random JS
   ↓
AtomicJS
   ↓
reference engine
   ↓
compare
```

---

# 53. Segurança

Adicionar:

- bounds checking;
- generation checked handles;
- no raw JS pointers;
- W^X;
- executable memory isolation;
- heap invariants;
- bytecode verifier;
- JIT validation em debug builds.

Nunca confiar no bytecode vindo do cache sem validação/versionamento.

---

# 54. Feature flags

Todas as grandes otimizações devem ser desligáveis:

```text
atomic.js.jit=false
atomic.js.baseline_jit=false
atomic.js.mid_tier=false
atomic.js.lazy_builtins=true
atomic.js.bytecode_cache=true
atomic.js.incremental_gc=false
```

Isso facilita benchmark A/B.

---

# 55. Fases de implementação

## Phase 0 — Skeleton

Objetivo:

```text
Rust workspace
Value
handles
error types
runtime bootstrap
```

Critério:

```text
empty script executes
```

---

## Phase 1 — Parser

Implementar:

```text
lexer
parser
AST
source positions
```

Critério:

```text
parse real JS programs
```

---

## Phase 2 — Bytecode

Implementar:

```text
AST -> bytecode
disassembler
verifier
```

Critério:

```text
simple scripts compile
```

---

## Phase 3 — Interpreter

Implementar:

```text
locals
arithmetic
branches
functions
calls
exceptions
closures
```

Critério:

```text
language test suite passes
```

---

## Phase 4 — Heap / GC

Implementar:

```text
nursery
allocation
minor GC
old generation
major GC
write barriers
```

Critério:

```text
stable under allocation stress
```

---

## Phase 5 — Object model

Implementar:

```text
Shape
property storage
prototypes
arrays
functions
```

Critério:

```text
real-world JS object patterns work
```

---

## Phase 6 — IC + profiler

Implementar:

```text
type feedback
shape feedback
property IC
call counters
loop counters
tiering
```

Critério:

```text
property access benchmark improves
```

---

## Phase 7 — Baseline JIT

Implementar:

```text
IR
Cranelift backend
CodeObject
safepoints
```

Critério:

```text
hot numeric/function workloads clearly outperform interpreter
```

---

## Phase 8 — Browser integration

Implementar:

```text
DOM
events
timers
fetch
console
modules
Promise
async
```

Critério:

```text
first real websites run
```

---

## Phase 9 — Startup optimization

Implementar:

```text
snapshot
lazy builtins
lazy functions
bytecode cache
streaming parser
background compilation
```

Critério:

```text
measurable startup improvement
```

---

## Phase 10 — Mid-tier JIT

Implementar:

```text
SSA IR
guards
specialization
basic inlining
```

Critério:

```text
hot loops and arithmetic materially improve
```

---

## Phase 11 — Deoptimization / OSR

Implementar:

```text
deopt metadata
state reconstruction
OSR
```

Critério:

```text
speculative optimization remains semantically correct
```

---

## Phase 12 — Advanced optimization

Somente mediante benchmark:

```text
better inlining
escape analysis
advanced LICM
constant propagation
loop optimization
SIMD
native code cache
WebAssembly
```

---

# 56. Ordem de prioridade real

Se os recursos forem limitados:

```text
1. Correct interpreter
2. GC
3. Objects / Shapes
4. Inline caches
5. Profiler
6. Baseline JIT
7. Lazy compilation
8. Bytecode cache
9. Snapshot
10. Background JIT
11. Mid-tier JIT
12. Deopt
13. OSR
14. Advanced optimizations
```

---

# 57. O que medir para decidir continuar

Depois do interpreter + GC + Shapes + IC:

Comparar:

```text
AtomicJS
QuickJS
V8 --jitless
V8 normal
```

Em:

```text
startup
RSS
heap
parse
compile
numeric loops
object access
function calls
array iteration
GC
idle-game simulation
```

### Sinal verde

Continuar para JIT se:

```text
AtomicJS startup < V8
AtomicJS memory < V8
AtomicJS cold JS >= QuickJS ou próximo
AtomicJS object-heavy workloads competitivos
```

### Sinal vermelho

Reavaliar arquitetura se:

```text
startup não melhora
memory não melhora
interpreter é muito mais lento que QuickJS
GC domina runtime
DOM bridge domina execução
```

O objetivo é validar a tese antes de investir meses no JIT.

---

# 58. Meta de arquitetura

A meta não é:

```text
AtomicJS = V8
```

A meta é:

```text
AtomicJS
    |
    +--> startup muito barato
    +--> memória baixa
    +--> execução cold eficiente
    +--> JIT barato
    +--> otimização seletiva
    +--> background-aware
    +--> browser-aware
```

---

# 59. Definição de sucesso

AtomicJS será considerado bem-sucedido quando:

1. Executar uma quantidade relevante de JavaScript real corretamente.
2. Conseguir rodar aplicações Web reais sem intervenção manual.
3. Tiver startup significativamente menor que uma engine generalista equivalente, em workloads relevantes.
4. Usar menos memória em cenários de várias abas.
5. Melhorar workloads quentes com baseline JIT.
6. Conseguir escalar para um mid-tier JIT sem reescrever VM/GC.
7. Manter todas as otimizações desligáveis.
8. Ter benchmarks e testes diferenciais suficientes para permitir evolução segura.

---

# 60. Instrução para implementação pelo Claude

Ao implementar AtomicJS:

1. Não inventar arquitetura paralela fora deste documento.
2. Antes de alterar uma camada, verificar as dependências existentes.
3. Não introduzir otimização prematura sem benchmark.
4. Não mover responsabilidades entre crates sem justificar a mudança.
5. Cada fase deve compilar e ter testes.
6. Cada otimização deve ter benchmark antes/depois.
7. Não implementar JIT antes de o interpreter estar semanticamente estável.
8. Não acoplar DOM ao core JavaScript.
9. Não usar ponteiros crus como identidade de objetos JS.
10. Preservar a possibilidade de desligar cada tier.
11. Preferir estruturas compactas e cache-friendly.
12. Não copiar código do V8; copiar apenas princípios arquiteturais e consultar documentação/código público para entender comportamento.

---

# 61. Primeiro milestone recomendado

Implementar somente:

```text
atomic-js-core
atomic-js-parser
atomic-js-bytecode
atomic-js-vm
atomic-js-object
atomic-js-gc
```

Com o seguinte programa funcionando:

```javascript
function sum(n) {
    let total = 0;

    for (let i = 0; i < n; i++) {
        total += i;
    }

    return total;
}

sum(1000000);
```

E também:

```javascript
const player = {
    level: 10,
    damage: 20
};

player.damage + player.level;
```

E:

```javascript
function makeCounter() {
    let count = 0;

    return function () {
        return ++count;
    };
}

const counter = makeCounter();

counter();
counter();
counter();
```

Só depois de esses casos estarem corretos:

```text
Shapes
↓
IC
↓
Profiler
↓
Baseline JIT
```

---

# 62. Regra de ouro

O AtomicJS deve sempre responder a esta pergunta antes de adicionar complexidade:

> "Isso reduz o custo total de executar JavaScript real no Atomic Browser?"

Se a resposta for não:

```text
não implementar ainda.
```
