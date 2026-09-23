/** Per-character material effects. All timing comes from the shared scheduler;
 * no independent timers, duplicate accessible labels, or CSS animation loops. */
export const SPECTACLE_TEXT_MODES = [
  "blackhole", "chrome", "plasma", "stained-glass", "aurora", "ripple", "hologram", "supernova", "matrix", "zoom", "ricochet", "corridor", "streak",
] as const;
export type SpectacleTextMode = typeof SPECTACLE_TEXT_MODES[number];

export function paintSpectacleChar(
  mode: SpectacleTextMode, el: HTMLElement, index: number, count: number,
  time: number, clickAge: number,
): void {
  const clock = time;
  time *= 2.6;
  const p = index * 0.65;
  const wave = Math.sin(time * 2 + p);
  const burst = Math.max(0, 1 - clickAge / 2);
  const offset = index - (count - 1) / 2;
  let transform = "";
  let shadow = "";
  let color = "";
  let opacity = 1;
  switch (mode) {
    case "zoom": {
      const lens = (Math.sin(clock * 4 - index * 0.45) + 1) / 2;
      transform = `scale(${0.75 + lens * 0.8}) rotate(${wave * 8}deg)`;
      shadow = `0 0 ${lens * 9}px currentColor`;
      break;
    }
    case "ricochet": {
      const phase = ((clock * 1.6 + index * 0.14) % 1);
      const bounce = 1 - Math.abs(phase * 2 - 1);
      transform = `translate(${Math.sin(clock * 4 + p) * 5}px, ${-bounce * 20}px) rotate(${wave * 18}deg) scale(${1.15 - bounce * 0.15}, ${0.85 + bounce * 0.15})`;
      shadow = `0 ${5 + bounce * 12}px 5px #0003`;
      break;
    }
    case "corridor": {
      const depth = (Math.sin(clock * 3 - p * 0.35) + 1) / 2;
      transform = `perspective(180px) translateZ(${depth * 38 - 20}px) rotateY(${Math.sin(clock * 2.2 + p * 0.3) * 35}deg)`;
      shadow = `${-offset * 0.7}px 2px 1px #0006, ${-offset * 1.4}px 4px 2px #0003`;
      opacity = 0.55 + depth * 0.45;
      break;
    }
    case "streak": {
      const speed = (Math.sin(clock * 4) + 1) / 2;
      transform = `translateX(${Math.sin(clock * 4 + p) * 5}px) scaleX(${1 + speed * 0.6})`;
      shadow = Array.from({length: 6}, (_, i) => `${-(i + 1) * (1 + speed * 3)}px 0 ${i * 0.4}px rgba(110,220,255,${(6 - i) * 0.09})`).join(",");
      color = "#e9faff";
      break;
    }
    case "matrix": {
      const fall = ((clock * 2.8 + index * 0.17) % 1);
      transform = `translateY(${(fall - 0.5) * 15}px)`;
      color = fall < 0.18 ? "#d3ffe2" : "#43ff7b";
      shadow = `0 -5px 2px #1bce6577, 0 -11px 4px #13854255, 0 0 7px #22ef70`;
      opacity = 0.55 + (1 - fall) * 0.45;
      break;
    }
    case "blackhole": {
      const pull = burst * Math.sin(Math.min(clickAge / 0.8, 1) * Math.PI);
      transform = `translate(${-offset * pull * 9}px, ${wave * (2 + pull * 8)}px) rotate(${Math.sin(clock * 1.8) * 12}deg) scale(${(1.15 + Math.cos(clock * 1.8) * 0.25) * (1 - pull * 0.7)}, ${(1.15 + Math.cos(clock * 1.8) * 0.25) * (1 + pull * 1.5)})`;
      color = `hsl(${35 - pull * 30}, 100%, ${85 - pull * 30}%)`;
      shadow = "0 0 7px #ff8c38, 0 0 15px #d83cff";
      break;
    }
    case "chrome":
      transform = `translateY(${wave * (2 + burst * 7)}px) scale(${1 + wave * 0.1}, ${1 - wave * 0.16})`;
      color = `hsl(200, ${15 + 20 * wave}%, ${65 + 30 * Math.sin(time * 2.5 + p)}%)`;
      shadow = "0 -1px 0 #fff, 0 2px 1px #122536, 2px 0 4px #43eaff, -2px 0 4px #f06eff";
      break;
    case "plasma": {
      const jitter = Math.sin(time * 19 + p * 9) * Math.sin(time * 7 + p);
      transform = `translate(${jitter * (1 + burst * 4)}px, ${wave}px)`;
      color = "#ecfaff";
      shadow = `0 0 2px white, ${wave * 4}px 0 6px #5cecff, ${-wave * 4}px 0 ${10 + burst * 10}px #b63cff`;
      break;
    }
    case "stained-glass": {
      const scatter = clickAge < 2 ? Math.sin(clickAge / 2 * Math.PI) : 0;
      transform = `translate(${offset * scatter * 9}px, ${Math.sin(p * 7) * scatter * 26}px) rotate(${Math.cos(p * 3) * scatter * 120 + wave * 8}deg) scale(${1 + wave * 0.12})`;
      color = `hsl(${index * 43 + time * 12}, 90%, 75%)`;
      shadow = "1px 1px 0 #182238, -1px -1px 0 #fff8, 0 0 6px currentColor";
      opacity = 1 - scatter * 0.25;
      break;
    }
    case "aurora":
      transform = `translateY(${wave * (3 + burst * 7)}px) skewX(${wave * 7}deg)`;
      color = `hsl(${155 + wave * 60}, 95%, 82%)`;
      shadow = `0 -4px 5px #37ffab, ${wave * 5}px -10px 10px #44cfff, ${-wave * 8}px -18px 16px #9d48ff`;
      break;
    case "ripple":
      transform = `translateY(${Math.sin(time * 3 - p) * (3 + burst * 9)}px) skewY(${wave * 9}deg)`;
      color = "#d0f8ff";
      shadow = `${wave * 3}px ${5 + wave * 2}px 2px #36c9f688, 0 0 8px #1b9fff`;
      break;
    case "hologram": {
      const slice = Math.max(0, Math.sin(time * 2.5 + p * 0.5)) ** 18;
      transform = `perspective(250px) translateX(${slice * (8 + burst * 12)}px) rotateY(${wave * 18}deg)`;
      color = "#adffff";
      shadow = `${2 + burst * 5}px 0 0 #ff3ba888, -2px 0 0 #3c8cff99, 0 0 7px #35ffff`;
      opacity = 0.8 + wave * 0.2;
      break;
    }
    case "supernova": {
      const kick = Math.exp(-((clock * 2.4) % 1) * 9);
      const shock = clickAge < 0.3 ? -clickAge : burst * Math.sin(Math.min((clickAge - 0.3) * 2, Math.PI));
      transform = `translate(${offset * shock * 5}px, ${wave * shock * 9}px) rotate(${Math.sin(clock * 3) * 7}deg) scale(${1 + kick * 0.45 + shock * 0.5})`;
      color = "#fff4c9";
      shadow = `0 0 3px white, 0 0 ${7 + burst * 8}px #ffbd35, ${wave * 3}px ${-5 - burst * 10}px 12px #ff4825`;
      break;
    }
  }
  el.style.transform = transform;
  el.style.textShadow = shadow;
  el.style.color = color;
  el.style.opacity = String(opacity);
}
