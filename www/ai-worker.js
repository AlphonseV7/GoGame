// AI worker: runs the (possibly slow) AI search off the main thread so the
// board and the Sensei avatar stay responsive while the AI “thinks”.
//
// The worker has its OWN WebAssembly instance with its own memory, so it cannot
// share the main thread's Game object. Instead the main thread sends the full
// move history; the worker replays it into a fresh Game and computes the reply.

import init, { Game } from './pkg/gogame.js';

let ready = false;

async function ensureReady() {
  if (!ready) {
    await init();
    ready = true;
  }
}

self.onmessage = async (e) => {
  const { size, diff, seed, history } = e.data;
  await ensureReady();

  // Rebuild the exact current position by replaying every move.
  const game = new Game(size);
  for (const idx of history) {
    if (idx < 0) {
      game.pass_turn();
    } else {
      game.place_stone(Math.floor(idx / size), idx % size);
    }
  }

  const move = game.get_ai_move(diff, seed);
  self.postMessage({ move });
};
