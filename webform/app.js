(() => {
  const canvas = document.querySelector('#particle-canvas');
  const ctx = canvas.getContext('2d');
  const state = {
    frame: 0,
    elapsed: 0,
    last: performance.now(),
    logicVersion: 1,
    generation: 0,
    palette: 0,
    world: '7F3A·9C21',
    anchor: '9f0c2a1b77',
    broken: false,
    particles: []
  };

  const palettes = [
    { name: 'white signal', core: [245, 245, 245], accent: [255, 255, 255], glow: 'rgba(255, 255, 255, .8)' },
    { name: 'silver motion', core: [178, 178, 178], accent: [235, 235, 235], glow: 'rgba(220, 220, 220, .8)' },
    { name: 'soft contrast', core: [215, 215, 215], accent: [150, 150, 150], glow: 'rgba(210, 210, 210, .75)' }
  ];

  const rand = (seed) => {
    let value = seed >>> 0;
    return () => {
      value = (value * 1664525 + 1013904223) >>> 0;
      return value / 4294967296;
    };
  };
  const random = rand(7);
  for (let i = 0; i < 460; i += 1) {
    const radius = .15 + random() * .9;
    const theta = random() * Math.PI * 2;
    const phi = Math.acos(2 * random() - 1);
    state.particles.push({
      x: Math.sin(phi) * Math.cos(theta) * radius,
      y: Math.sin(phi) * Math.sin(theta) * radius,
      z: Math.cos(phi) * radius,
      vx: (random() - .5) * .16,
      vy: (random() - .5) * .16,
      vz: (random() - .5) * .16,
      phase: random() * Math.PI * 2,
      size: .6 + random() * 2.1
    });
  }

  const el = (id) => document.getElementById(id);
  const ui = {
    process: el('process-id'), frame: el('frame-count'), elapsed: el('elapsed-time'), world: el('world-id'), anchor: el('anchor-id'),
    fill: el('state-bar-fill'), version: el('logic-version'), logicName: el('logic-name'), colorName: el('logic-color-name'), generation: el('generation-label'),
    log: el('demo-log'), canvas: canvas, reload: el('reload-button'), breakButton: el('break-button')
  };

  function resize() {
    const ratio = Math.min(window.devicePixelRatio || 1, 2);
    const rect = canvas.getBoundingClientRect();
    canvas.width = Math.max(1, Math.floor(rect.width * ratio));
    canvas.height = Math.max(1, Math.floor(rect.height * ratio));
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
  }
  window.addEventListener('resize', resize);
  resize();

  const formatFrame = (value) => String(value).padStart(6, '0');
  const formatElapsed = (value) => {
    const minutes = Math.floor(value / 60).toString().padStart(2, '0');
    const seconds = (value % 60).toFixed(1).padStart(4, '0');
    return `${minutes}:${seconds}`;
  };
  const timeNow = () => new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  function writeLog(message, failed = false) {
    ui.log.innerHTML = `<span class="log-time">${timeNow()}</span> <span class="${failed ? 'log-fail' : 'log-ok'}">●</span> ${message}`;
  }
  function updateTelemetry() {
    ui.process.textContent = `pid ${18472}`;
    ui.frame.textContent = formatFrame(state.frame);
    ui.elapsed.textContent = formatElapsed(state.elapsed);
    ui.world.textContent = state.world;
    ui.anchor.textContent = state.anchor;
    ui.version.textContent = `V${state.logicVersion}`;
    ui.generation.textContent = `generation ${state.generation}`;
    ui.colorName.textContent = palettes[state.palette].name;
    ui.fill.style.width = `${60 + Math.min(35, (state.frame % 900) / 25)}%`;
  }
  function reloadBehavior() {
    state.logicVersion += 1;
    state.generation += 1;
    state.palette = (state.palette + 1) % palettes.length;
    state.broken = false;
    ui.logicName.textContent = 'particle_logic.py';
    ui.reload.classList.add('flash');
    setTimeout(() => ui.reload.classList.remove('flash'), 500);
    writeLog(`generation ${state.generation} activated — state preserved`);
    updateTelemetry();
  }
  function breakCandidate() {
    state.broken = true;
    writeLog('candidate rejected — last good generation remains active', true);
    ui.breakButton.classList.add('flash-red');
    setTimeout(() => ui.breakButton.classList.remove('flash-red'), 500);
  }
  ui.reload.addEventListener('click', reloadBehavior);
  ui.breakButton.addEventListener('click', breakCandidate);

  function draw(now) {
    const delta = Math.min(.05, Math.max(0, (now - state.last) / 1000));
    state.last = now;
    state.elapsed += delta;
    state.frame += 1;
    const rect = canvas.getBoundingClientRect();
    const width = rect.width;
    const height = rect.height;
    const palette = palettes[state.palette];
    const [r, g, b] = palette.core;
    ctx.clearRect(0, 0, width, height);
    const gradient = ctx.createRadialGradient(width * .5, height * .5, 4, width * .5, height * .5, Math.max(width, height) * .62);
    gradient.addColorStop(0, `rgba(${r}, ${g}, ${b}, .09)`);
    gradient.addColorStop(.54, 'rgba(10, 18, 34, .15)');
    gradient.addColorStop(1, 'rgba(5, 9, 16, 0)');
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, width, height);

    const spin = state.elapsed * (state.logicVersion === 1 ? .42 : state.logicVersion === 2 ? .9 : 1.35);
    const projected = [];
    state.particles.forEach((p) => {
      const force = state.logicVersion === 1 ? 1 : state.logicVersion === 2 ? 1.15 : .8;
      const swirl = state.logicVersion === 1 ? .28 : state.logicVersion === 2 ? .62 : .15;
      p.vx += (-p.x * .11 + p.y * swirl) * delta * force;
      p.vy += (-p.y * .11 - p.x * swirl) * delta * force;
      p.vz += (-p.z * .08 + Math.sin(state.elapsed * 1.8 + p.phase) * .03) * delta;
      p.vx *= .998; p.vy *= .998; p.vz *= .998;
      p.x += p.vx * delta; p.y += p.vy * delta; p.z += p.vz * delta;
      const radius = Math.hypot(p.x, p.y, p.z);
      if (radius > 1.25) { p.x *= .96; p.y *= .96; p.z *= .96; }
      const x = p.x * Math.cos(spin) - p.z * Math.sin(spin);
      const z = p.x * Math.sin(spin) + p.z * Math.cos(spin);
      const perspective = 1 / (1.8 - z * .68);
      projected.push({ x: width / 2 + x * width * .31 * perspective, y: height / 2 + p.y * height * .38 * perspective, z, size: p.size * perspective, depth: perspective });
    });
    projected.sort((a, b2) => a.z - b2.z);
    projected.forEach((p) => {
      const alpha = Math.max(.15, Math.min(.94, .28 + p.depth * .42));
      ctx.beginPath();
      ctx.fillStyle = `rgba(${r}, ${g}, ${b}, ${alpha})`;
      ctx.arc(p.x, p.y, p.size, 0, Math.PI * 2);
      ctx.fill();
    });
    const core = ctx.createRadialGradient(width / 2, height / 2, 0, width / 2, height / 2, 110);
    core.addColorStop(0, `rgba(${palette.accent[0]}, ${palette.accent[1]}, ${palette.accent[2]}, .1)`);
    core.addColorStop(1, 'rgba(0,0,0,0)');
    ctx.fillStyle = core;
    ctx.fillRect(width / 2 - 115, height / 2 - 115, 230, 230);
    updateTelemetry();
    requestAnimationFrame(draw);
  }
  requestAnimationFrame(draw);

  document.querySelectorAll('.code-tab').forEach((button) => {
    button.addEventListener('click', () => {
      document.querySelectorAll('.code-tab').forEach((item) => item.classList.remove('active'));
      document.querySelectorAll('.code-content').forEach((item) => item.classList.remove('active'));
      button.classList.add('active');
      document.querySelector(`.code-content[data-code="${button.dataset.tab}"]`).classList.add('active');
      document.querySelector('.code-file').textContent = button.dataset.tab === 'python' ? 'app.py' : 'host.c';
    });
  });

  const toast = el('toast');
  function showToast(message) { toast.textContent = message; toast.classList.add('show'); setTimeout(() => toast.classList.remove('show'), 1800); }
  document.getElementById('copy-code').addEventListener('click', async () => {
    const active = document.querySelector('.code-content.active');
    try { await navigator.clipboard.writeText(active.innerText); showToast('Code copied'); } catch { showToast('Select the code to copy'); }
  });
  document.querySelector('.copy-command').addEventListener('click', async (event) => {
    const command = event.currentTarget.dataset.command;
    try { await navigator.clipboard.writeText(command); showToast('Command copied'); } catch { showToast(command); }
  });
})();
