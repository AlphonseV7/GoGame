import init, { Game } from './pkg/gogame.js';

// ── State ────────────────────────────────────────────────
let game      = null;
let gameMode  = 'pvp';  // 'pvp' | 'pvai'
let aiDiff    = 0;       // 0=noob 1=average 2=dan
let boardSize = 19;
let aiThinking = false;

let canvas, ctx, cellSize;
const PAD = 38;
const HOSHI = { 9:[2,4,6], 13:[3,6,9], 19:[3,9,15] };

// ── Boot ─────────────────────────────────────────────────
async function boot() {
  await init();
  bindAll();
  show('screen-title');
}

// ── Screen routing ───────────────────────────────────────
function show(id) {
  document.querySelectorAll('.screen').forEach(s => s.classList.add('hidden'));
  document.getElementById(id).classList.remove('hidden');
}
function $(id) { return document.getElementById(id); }

// ── Wire up all buttons ──────────────────────────────────
function bindAll() {
  // Title
  $('btn-local-game').onclick = () => show('screen-local-setup');

  // Local setup
  $('btn-pvai').onclick = () => { gameMode = 'pvai'; show('screen-ai-difficulty'); };
  $('btn-pvp').onclick  = () => { gameMode = 'pvp';  show('screen-board-size'); };
  $('btn-back-local').onclick = () => show('screen-title');

  // AI difficulty
  document.querySelectorAll('[data-diff]').forEach(btn =>
    btn.addEventListener('click', () => {
      aiDiff = parseInt(btn.dataset.diff);
      show('screen-board-size');
    })
  );
  $('btn-back-diff').onclick = () => show('screen-local-setup');

  // Board size
  document.querySelectorAll('[data-size]').forEach(btn =>
    btn.addEventListener('click', () => {
      boardSize = parseInt(btn.dataset.size);
      startGame();
    })
  );
  $('btn-back-size').onclick = () =>
    show(gameMode === 'pvai' ? 'screen-ai-difficulty' : 'screen-local-setup');

  // In-game
  $('btn-menu').onclick = () => show('screen-title');
  $('pass-btn').onclick = () => {
    if (aiThinking) return;
    game.pass_turn();
    update();
    maybeAI();
  };
  $('new-game-btn').onclick = () => show('screen-title');
}

// ── Start a game ─────────────────────────────────────────
function startGame() {
  aiThinking = false;
  game = new Game(boardSize);
  show('screen-game');
  $('game-over-overlay').classList.add('hidden');
  setupCanvas();
  update();
}

// ── Canvas ───────────────────────────────────────────────
function setupCanvas() {
  canvas = $('board-canvas');
  ctx = canvas.getContext('2d');
  canvas.onclick = onCanvasClick;
  const maxPx = Math.min(window.innerWidth - 48, window.innerHeight - 180, 580);
  canvas.width = canvas.height = maxPx;
  cellSize = (maxPx - PAD * 2) / (boardSize - 1);
}

function px(i) { return PAD + i * cellSize; }

// ── Draw ─────────────────────────────────────────────────
function draw() {
  const s = canvas.width;
  ctx.fillStyle = '#c8a84b';
  ctx.fillRect(0, 0, s, s);

  // Wood grain
  ctx.save();
  ctx.globalAlpha = 0.055;
  for (let i = -s; i < s * 2; i += 10) {
    ctx.strokeStyle = '#7a5010'; ctx.lineWidth = 2;
    ctx.beginPath(); ctx.moveTo(i, 0); ctx.lineTo(i + s * 0.35, s); ctx.stroke();
  }
  ctx.restore();

  // Grid
  ctx.strokeStyle = '#5a3e1b'; ctx.lineWidth = 1;
  for (let i = 0; i < boardSize; i++) {
    ctx.beginPath(); ctx.moveTo(px(i), px(0)); ctx.lineTo(px(i), px(boardSize-1)); ctx.stroke();
    ctx.beginPath(); ctx.moveTo(px(0), px(i)); ctx.lineTo(px(boardSize-1), px(i)); ctx.stroke();
  }

  // Hoshi
  const dots = HOSHI[boardSize] || [];
  dots.forEach(r => dots.forEach(c => {
    ctx.beginPath(); ctx.arc(px(r), px(c), 4, 0, Math.PI*2);
    ctx.fillStyle = '#5a3e1b'; ctx.fill();
  }));

  // Stones
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

// ── Input ────────────────────────────────────────────────
function onCanvasClick(e) {
  if (aiThinking || game.is_game_over()) return;
  if (gameMode === 'pvai' && game.current_player() === 2) return;
  const rect = canvas.getBoundingClientRect();
  const sx = canvas.width / rect.width, sy = canvas.height / rect.height;
  const col = Math.round(((e.clientX - rect.left)*sx - PAD) / cellSize);
  const row = Math.round(((e.clientY - rect.top )*sy - PAD) / cellSize);
  if (row < 0 || row >= boardSize || col < 0 || col >= boardSize) return;
  if (game.place_stone(row, col)) { update(); maybeAI(); }
}

// ── Update UI ────────────────────────────────────────────
function update() {
  draw();
  const p = game.current_player();
  $('turn-text').textContent = p === 1 ? "Black’s Turn" : "White’s Turn";
  $('black-captures').textContent = `${game.black_captures()} cap`;
  $('white-captures').textContent = `${game.white_captures()} cap`;
  if (game.is_game_over()) {
    $('result-text').textContent = 'Both players passed. Count the territory!';
    $('game-over-overlay').classList.remove('hidden');
  }
}

// ── AI ───────────────────────────────────────────────────
function maybeAI() {
  if (gameMode !== 'pvai' || game.is_game_over()) return;
  if (game.current_player() !== 2) return; // White = AI
  aiThinking = true;
  $('pass-btn').disabled = true;
  $('ai-status').textContent = 'thinking…';
  $('turn-text').textContent = 'AI thinking…';
  setTimeout(() => {
    const seed = (Date.now() & 0xFFFFFFFF) >>> 0;
    const idx = game.get_ai_move(aiDiff, seed);
    if (idx === -1) {
      game.pass_turn();
    } else {
      const row = Math.floor(idx / boardSize);
      const col = idx % boardSize;
      game.place_stone(row, col);
    }
    aiThinking = false;
    $('pass-btn').disabled = false;
    $('ai-status').textContent = '';
    update();
  }, 150);
}

boot().catch(console.error);
