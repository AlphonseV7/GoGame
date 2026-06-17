import init, { Game } from './pkg/gogame.js';

const BOARD_SIZE = 19;
const PADDING = 38;
const LINE_COLOR = '#5a3e1b';
const BOARD_COLOR = '#c8a84b';
const HOSHI = [3, 9, 15];

let game, canvas, ctx, cellSize;

async function main() {
  await init();
  game = new Game();

  canvas = document.getElementById('board-canvas');
  ctx = canvas.getContext('2d');

  resize();
  window.addEventListener('resize', () => { resize(); draw(); });
  canvas.addEventListener('click', onClick);
  document.getElementById('pass-btn').addEventListener('click', () => { game.pass_turn(); update(); });
  document.getElementById('reset-btn').addEventListener('click', reset);
  document.getElementById('new-game-btn').addEventListener('click', reset);

  draw();
}

function resize() {
  const maxSize = Math.min(window.innerWidth - 48, window.innerHeight - 220, 580);
  canvas.width  = maxSize;
  canvas.height = maxSize;
  cellSize = (maxSize - PADDING * 2) / (BOARD_SIZE - 1);
}

function px(i) {
  return PADDING + i * cellSize;
}

function draw() {
  const s = canvas.width;
  ctx.clearRect(0, 0, s, s);

  // Board background
  ctx.fillStyle = BOARD_COLOR;
  ctx.fillRect(0, 0, s, s);

  // Subtle wood grain
  ctx.save();
  ctx.globalAlpha = 0.06;
  for (let i = -s; i < s * 2; i += 10) {
    ctx.strokeStyle = '#7a5010';
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(i, 0);
    ctx.lineTo(i + s * 0.4, s);
    ctx.stroke();
  }
  ctx.restore();

  // Grid
  ctx.strokeStyle = LINE_COLOR;
  ctx.lineWidth = 1;
  for (let i = 0; i < BOARD_SIZE; i++) {
    ctx.beginPath(); ctx.moveTo(px(i), px(0)); ctx.lineTo(px(i), px(BOARD_SIZE - 1)); ctx.stroke();
    ctx.beginPath(); ctx.moveTo(px(0), px(i)); ctx.lineTo(px(BOARD_SIZE - 1), px(i)); ctx.stroke();
  }

  // Hoshi (star points)
  HOSHI.forEach(r => HOSHI.forEach(c => {
    ctx.beginPath();
    ctx.arc(px(r), px(c), 4, 0, Math.PI * 2);
    ctx.fillStyle = LINE_COLOR;
    ctx.fill();
  }));

  // Stones
  for (let r = 0; r < BOARD_SIZE; r++) {
    for (let c = 0; c < BOARD_SIZE; c++) {
      const cell = game.get_cell(r, c);
      if (cell !== 0) drawStone(r, c, cell === 1 ? 'black' : 'white');
    }
  }
}

function drawStone(row, col, color) {
  const x = px(col);
  const y = px(row);
  const r = cellSize * 0.46;

  const g = ctx.createRadialGradient(x - r * 0.28, y - r * 0.28, r * 0.08, x, y, r);
  if (color === 'black') {
    g.addColorStop(0, '#707070');
    g.addColorStop(0.35, '#282828');
    g.addColorStop(1, '#080808');
  } else {
    g.addColorStop(0, '#ffffff');
    g.addColorStop(0.5, '#f0ede8');
    g.addColorStop(1, '#c0bdb7');
  }

  // Drop shadow
  ctx.save();
  ctx.shadowColor = 'rgba(0,0,0,0.4)';
  ctx.shadowBlur = 5;
  ctx.shadowOffsetX = 2;
  ctx.shadowOffsetY = 3;
  ctx.beginPath();
  ctx.arc(x, y, r, 0, Math.PI * 2);
  ctx.fillStyle = g;
  ctx.fill();
  ctx.restore();

  if (color === 'white') {
    ctx.beginPath();
    ctx.arc(x, y, r, 0, Math.PI * 2);
    ctx.strokeStyle = 'rgba(0,0,0,0.15)';
    ctx.lineWidth = 0.7;
    ctx.stroke();
  }
}

function onClick(e) {
  if (game.is_game_over()) return;
  const rect = canvas.getBoundingClientRect();
  const scaleX = canvas.width  / rect.width;
  const scaleY = canvas.height / rect.height;
  const x = (e.clientX - rect.left) * scaleX;
  const y = (e.clientY - rect.top)  * scaleY;
  const col = Math.round((x - PADDING) / cellSize);
  const row = Math.round((y - PADDING) / cellSize);
  if (row < 0 || row >= BOARD_SIZE || col < 0 || col >= BOARD_SIZE) return;
  if (game.place_stone(row, col)) update();
}

function update() {
  draw();
  const p = game.current_player();
  document.getElementById('turn-text').textContent = p === 1 ? "Black's Turn" : "White's Turn";
  document.getElementById('black-captures').textContent = `${game.black_captures()} captures`;
  document.getElementById('white-captures').textContent = `${game.white_captures()} captures`;
  if (game.is_game_over()) {
    document.getElementById('game-over-overlay').classList.remove('hidden');
  }
}

function reset() {
  game = new Game();
  document.getElementById('game-over-overlay').classList.add('hidden');
  document.getElementById('turn-text').textContent = "Black's Turn";
  document.getElementById('black-captures').textContent = '0 captures';
  document.getElementById('white-captures').textContent = '0 captures';
  draw();
}

main().catch(console.error);
