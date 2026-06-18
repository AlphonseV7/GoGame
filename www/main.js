import init, { Game } from './pkg/gogame.js';

// ── State ──
let game      = null;
let gameMode  = 'pvp';   // 'pvp' | 'pvai'
let aiDiff    = 0;        // 0=noob 1=average 2=dan
let boardSize = 19;
let aiThinking = false;
let history   = [];       // every move applied, as index (row*size+col) or -1 for pass

let worker = null;        // AI Web Worker (null → fall back to main-thread AI)

let canvas, ctx, cellSize;
const PAD = 38;
const HOSHI = { 9:[2,4,6], 13:[3,6,9], 19:[3,9,15] };

const DIFF_NAMES = ['Noob', 'Average', 'Dan'];

// ── Boot ──
async function boot() {
  await init();
  setupWorker();
  bindAll();
  show('screen-title');
}

function setupWorker() {
  try {
    worker = new Worker(new URL('./ai-worker.js', import.meta.url), { type: 'module' });
    worker.onmessage = (e) => applyAIMove(e.data.move);
    worker.onerror = () => { worker = null; }; // fall back to synchronous AI
  } catch (_) {
    worker = null;
  }
}

// ── Screen routing ──
function show(id) {
  document.querySelectorAll('.screen').forEach(s => s.classList.add('hidden'));
  document.getElementById(id).classList.remove('hidden');
}
function $(id) { return document.getElementById(id); }

// ── Buttons ──
function bindAll() {
  $('btn-local-game').onclick = () => show('screen-local-setup');

  $('btn-pvai').onclick = () => { gameMode = 'pvai'; show('screen-ai-difficulty'); };
  $('btn-pvp').onclick  = () => { gameMode = 'pvp';  show('screen-board-size'); };
  $('btn-back-local').onclick = () => show('screen-title');

  document.querySelectorAll('[data-diff]').forEach(btn =>
    btn.addEventListener('click', () => {
      aiDiff = parseInt(btn.dataset.diff);
      show('screen-board-size');
    })
  );
  $('btn-back-diff').onclick = () => show('screen-local-setup');

  document.querySelectorAll('[data-size]').forEach(btn =>
    btn.addEventListener('click', () => {
      boardSize = parseInt(btn.dataset.size);
      startGame();
    })
  );
  $('btn-back-size').onclick = () =>
    show(gameMode === 'pvai' ? 'screen-ai-difficulty' : 'screen-local-setup');

  $('btn-menu').onclick = () => show('screen-title');
  $('pass-btn').onclick = () => {
    if (aiThinking || game.is_game_over()) return;
    game.pass_turn();
    history.push(-1);
    update();
    maybeAI();
  };
  $('new-game-btn').onclick = () => show('screen-title');
}

// ── Start a game ──
function startGame() {
  aiThinking = false;
  history = [];
  game = new Game(boardSize);
  show('screen-game');
  $('game-over-panel').classList.add('hidden');
  // Show the Sensei avatar only when playing against the AI.
  $('ai-avatar-wrap').classList.toggle('hidden', gameMode !== 'pvai');
  $('avatar-name').textContent = gameMode === 'pvai' ? `Sensei · ${DIFF_NAMES[aiDiff]}` : '';
  // In PvP the left avatar is Black, in PvAI it's the human player.
  $('player-avatar-wrap').querySelector('.side-name').textContent =
    gameMode === 'pvai' ? 'You' : 'Black';
  setThinking(false);
  setupCanvas();
  update();
}

// ── Canvas ──
function setupCanvas() {
  canvas = $('board-canvas');
  ctx = canvas.getContext('2d');
  canvas.onclick = onCanvasClick;
  // Leave room for the avatars flanking the board (~76px each side).
  const maxPx = Math.min(window.innerWidth - 48 - 152, window.innerHeight - 220, 520);
  canvas.width = canvas.height = Math.max(maxPx, 240);
  cellSize = (maxPx - PAD * 2) / (boardSize - 1);
}

function px(i) { return PAD + i * cellSize; }

// ── Draw ──
function draw() {
  const s = canvas.width;
  ctx.fillStyle = '#c8a84b';
  ctx.fillRect(0, 0, s, s);

  ctx.save();
  ctx.globalAlpha = 0.055;
  for (let i = -s; i < s * 2; i += 10) {
    ctx.strokeStyle = '#7a5010'; ctx.lineWidth = 2;
    ctx.beginPath(); ctx.moveTo(i, 0); ctx.lineTo(i + s * 0.35, s); ctx.stroke();
  }
  ctx.restore();

  ctx.strokeStyle = '#5a3e1b'; ctx.lineWidth = 1;
  for (let i = 0; i < boardSize; i++) {
    ctx.beginPath(); ctx.moveTo(px(i), px(0)); ctx.lineTo(px(i), px(boardSize-1)); ctx.stroke();
    ctx.beginPath(); ctx.moveTo(px(0), px(i)); ctx.lineTo(px(boardSize-1), px(i)); ctx.stroke();
  }

  const dots = HOSHI[boardSize] || [];
  dots.forEach(r => dots.forEach(c => {
    ctx.beginPath(); ctx.arc(px(r), px(c), 4, 0, Math.PI*2);
    ctx.fillStyle = '#5a3e1b'; ctx.fill();
  }));

  for (let r = 0; r < boardSize; r++)
    for (let c = 0; c < boardSize; c++) {
      const cell = game.get_cell(r, c);
      if (cell !== 0) drawStone(r, c, cell === 1 ? 'black' : 'white');
    }
}

function drawStone(row, col, color) {
  const x = px(col), y = px(row), r = cellSize * 0.46;
  const g = ctx.createRadialGradient(x - r*.28, y - r*.28, r*.08, x, y, r);
  if (color === 'black') {
    g.addColorStop(0, '#707070'); g.addColorStop(.35, '#282828'); g.addColorStop(1, '#080808');
  } else {
    g.addColorStop(0, '#ffffff'); g.addColorStop(.5, '#f0ede8'); g.addColorStop(1, '#c0bdb7');
  }
  ctx.save();
  ctx.shadowColor = 'rgba(0,0,0,.38)'; ctx.shadowBlur = 5;
  ctx.shadowOffsetX = 2; ctx.shadowOffsetY = 3;
  ctx.beginPath(); ctx.arc(x, y, r, 0, Math.PI*2);
  ctx.fillStyle = g; ctx.fill();
  ctx.restore();
  if (color === 'white') {
    ctx.beginPath(); ctx.arc(x, y, r, 0, Math.PI*2);
    ctx.strokeStyle = 'rgba(0,0,0,.14)'; ctx.lineWidth = .7; ctx.stroke();
  }
}

// ── Input ──
function onCanvasClick(e) {
  if (aiThinking || game.is_game_over()) return;
  if (gameMode === 'pvai' && game.current_player() === 2) return; // AI is White
  const rect = canvas.getBoundingClientRect();
  const sx = canvas.width / rect.width, sy = canvas.height / rect.height;
  const col = Math.round(((e.clientX - rect.left)*sx - PAD) / cellSize);
  const row = Math.round(((e.clientY - rect.top )*sy - PAD) / cellSize);
  if (row < 0 || row >= boardSize || col < 0 || col >= boardSize) return;
  if (game.place_stone(row, col)) {
    history.push(row * boardSize + col);
    update();
    maybeAI();
  }
}

// ── Update UI ──
function update() {
  draw();
  const p = game.current_player();
  $('turn-text').textContent = p === 1 ? "Black’s Turn" : "White’s Turn";

  const bc = game.black_captures(), wc = game.white_captures();
  $('black-captures').textContent = bc;
  $('white-captures').textContent = wc;
  renderCapDots('black-cap-dots', bc, 'black');
  renderCapDots('white-cap-dots', wc, 'white');

  if (game.is_game_over()) {
    showScore();
    $('game-over-panel').classList.remove('hidden');
  }
}

// Draw one small stone per capture, in the capturing player's colour.
// They wrap onto a new row after 10 (see .cap-dots max-width in CSS).
function renderCapDots(id, count, color) {
  const el = $(id);
  el.innerHTML = '';
  for (let i = 0; i < count; i++) {
    const s = document.createElement('span');
    s.className = `mini-stone ${color}`;
    el.appendChild(s);
  }
}

// Format a score: whole numbers stay clean, half-points show one decimal.
function fmt(n) { return Number.isInteger(n) ? `${n}` : n.toFixed(1); }

function showScore() {
  const bt = game.black_territory(), wt = game.white_territory();
  const bc = game.black_captures(),  wc = game.white_captures();
  const komi = game.komi();
  const bs = game.black_score(),     ws = game.white_score();

  $('sc-black-terr').textContent  = bt;
  $('sc-black-pris').textContent  = bc;
  $('sc-black-total').textContent = fmt(bs);
  $('sc-white-terr').textContent  = wt;
  $('sc-white-pris').textContent  = wc;
  $('sc-white-komi').textContent  = fmt(komi);
  $('sc-white-total').textContent = fmt(ws);

  const w = game.winner();
  const margin = fmt(Math.abs(bs - ws));
  $('result-text').textContent =
    w === 1 ? `Black wins by ${margin}` :
    w === 2 ? `White wins by ${margin}` :
              'Dead even — a tie!';
}

// ── Avatar “thinking” state ──
function setThinking(on) {
  $('ai-avatar-wrap').classList.toggle('thinking', on);
  $('ai-status').textContent = on ? 'thinking…' : '';
}

// ── AI turn ──
function maybeAI() {
  if (gameMode !== 'pvai' || game.is_game_over()) return;
  if (game.current_player() !== 2) return; // White = AI
  aiThinking = true;
  $('pass-btn').disabled = true;
  setThinking(true);

  const seed = (Date.now() & 0xFFFFFFFF) >>> 0;
  if (worker) {
    worker.postMessage({ size: boardSize, diff: aiDiff, seed, history: history.slice() });
  } else {
    // Fallback: compute on the main thread (briefly freezes the page).
    setTimeout(() => applyAIMove(game.get_ai_move(aiDiff, seed)), 50);
  }
}

function applyAIMove(move) {
  if (move === -1) {
    game.pass_turn();
    history.push(-1);
  } else {
    const row = Math.floor(move / boardSize);
    const col = move % boardSize;
    game.place_stone(row, col);
    history.push(move);
  }
  aiThinking = false;
  $('pass-btn').disabled = false;
  setThinking(false);
  update();
}

boot().catch(console.error);
