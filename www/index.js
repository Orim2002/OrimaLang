/**
 * OrimaLang Web Playground
 *
 * Loads the WASM module produced by:
 *   wasm-pack build --target web --features wasm --out-dir www/pkg
 *
 * Then wires up the Run / Clear / Example buttons.
 */

const EXAMPLE_PROGRAM = `note ── OrimaLang example program ──

set name to Ori.
set score to 0.
create list fruits.
add apple to fruits.
add banana to fruits.
add cherry to fruits.

say welcome and name.

for each fruit in fruits, say fruit, end for.

repeat 3 times, increase score by 10, end repeat.
say your score is and score.

if score is at least 25, say great job, otherwise say keep trying, end if.

define greet taking person, say hello and person, end define.
run greet with World.

set msg to hello.
set msg to msg in uppercase.
say msg.
`;

const runBtn     = document.getElementById('runBtn');
const clearBtn   = document.getElementById('clearBtn');
const exampleBtn = document.getElementById('exampleBtn');
const codeArea   = document.getElementById('code');
const outputDiv  = document.getElementById('output');
const statusEl   = document.getElementById('status');

let wasmModule = null;

async function loadWasm() {
  try {
    // The wasm-pack --target web output lives at ./pkg/orimalang.js
    const mod = await import('./pkg/orimalang.js');
    await mod.default(); // initialise the WASM binary
    wasmModule = mod;
    statusEl.textContent = 'WASM ready.';
    runBtn.disabled = false;
  } catch (err) {
    statusEl.textContent = 'WASM not loaded (build with wasm-pack first).';
    console.warn('WASM load failed:', err);
    // Provide a graceful fallback message in the output pane
    outputDiv.textContent = [
      'WASM module not found.',
      '',
      'To build the playground:',
      '  wasm-pack build --target web --features wasm --out-dir www/pkg',
      '',
      'Then serve this directory with any static HTTP server.',
    ].join('\n');
  }
}

function runCode() {
  if (!wasmModule) {
    outputDiv.textContent = 'WASM module is not loaded yet.';
    outputDiv.classList.add('has-error');
    return;
  }
  const source = codeArea.value;
  if (!source.trim()) {
    outputDiv.textContent = '(no code to run)';
    outputDiv.classList.remove('has-error');
    return;
  }
  try {
    const result = wasmModule.run_program(source);
    outputDiv.textContent = result || '(no output)';
    outputDiv.classList.toggle('has-error', result.startsWith('OrimaLang Error'));
  } catch (err) {
    outputDiv.textContent = `Runtime error: ${err}`;
    outputDiv.classList.add('has-error');
  }
}

runBtn.addEventListener('click', runCode);

clearBtn.addEventListener('click', () => {
  outputDiv.textContent = '';
  outputDiv.classList.remove('has-error');
});

exampleBtn.addEventListener('click', () => {
  codeArea.value = EXAMPLE_PROGRAM;
  outputDiv.textContent = '';
  outputDiv.classList.remove('has-error');
});

// Allow Ctrl+Enter to run from the textarea
codeArea.addEventListener('keydown', (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
    e.preventDefault();
    runCode();
  }
});

// Disable the run button until WASM is ready
runBtn.disabled = true;
loadWasm();
