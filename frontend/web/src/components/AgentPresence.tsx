import { useEffect, useRef, useState } from "react";

type AgentPresenceProps = {
  busy?: boolean;
};

type WalkMode = "idle" | "walk";

const BODY_WIDTH = 150;
const EDGE_PAD = 8;
const WALK_FRAMES = 26;
/** Bump when replacing /brand walk sprites so browsers don't keep stale webp. */
const ASSET_REV = "walk26-ltr-slow4";

function brandUrl(path: string) {
  return `${path}?v=${ASSET_REV}`;
}


function randomBetween(min: number, max: number) {
  return min + Math.random() * (max - min);
}

/** Walk lane: chat left → composer left (inside .chatStage). */
function measureWalkBounds(stage: HTMLElement): { minX: number; maxX: number } {
  const minX = EDGE_PAD;
  const stageRect = stage.getBoundingClientRect();
  const chatStage = stage.closest(".chatStage") ?? stage.parentElement;
  const composer = chatStage?.querySelector(".composer") as HTMLElement | null;
  if (!composer) {
    return { minX, maxX: Math.max(minX, stage.clientWidth * 0.45 - BODY_WIDTH) };
  }
  const composerRect = composer.getBoundingClientRect();
  const composerLeftInStage = composerRect.left - stageRect.left;
  const maxX = Math.max(minX, composerLeftInStage - BODY_WIDTH - EDGE_PAD);
  return { minX, maxX };
}

export function AgentPresence({ busy = false }: AgentPresenceProps) {
  const stageRef = useRef<HTMLElement | null>(null);
  const xRef = useRef(24);
  const boundsRef = useRef({ minX: EDGE_PAD, maxX: 200 });
  const [x, setX] = useState(24);
  const [facing, setFacing] = useState<1 | -1>(1);
  const [mode, setMode] = useState<WalkMode>("idle");
  const [frame, setFrame] = useState(0);
  const targetRef = useRef(24);
  const modeRef = useRef<WalkMode>("idle");
  const busyRef = useRef(busy);
  const timerRef = useRef<number | null>(null);
  const rafRef = useRef<number | null>(null);

  busyRef.current = busy;

  useEffect(() => {
    for (let i = 0; i < WALK_FRAMES; i += 1) {
      const img = new Image();
      img.src = brandUrl(`/brand/agent-walk-${i}.webp`);
    }
  }, []);

  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;

    const refreshBounds = () => {
      boundsRef.current = measureWalkBounds(stage);
      const { minX, maxX } = boundsRef.current;
      const capped = Math.min(Math.max(xRef.current, minX), maxX);
      xRef.current = capped;
      setX(capped);
      targetRef.current = Math.min(Math.max(targetRef.current, minX), maxX);
    };

    const clearTimer = () => {
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };

    const startWalkTo = (next: number) => {
      const { minX, maxX } = boundsRef.current;
      const clamped = Math.min(Math.max(next, minX), maxX);
      targetRef.current = clamped;
      modeRef.current = "walk";
      setMode("walk");
      setFacing(clamped >= xRef.current ? 1 : -1);
    };

    const scheduleIdle = () => {
      clearTimer();
      modeRef.current = "idle";
      setMode("idle");
      setFrame(0);
      const pause = busyRef.current ? randomBetween(400, 900) : randomBetween(1400, 3600);
      timerRef.current = window.setTimeout(() => {
        refreshBounds();
        const { minX, maxX } = boundsRef.current;
        startWalkTo(randomBetween(minX, Math.max(minX, maxX)));
      }, pause);
    };

    refreshBounds();
    const { minX, maxX } = boundsRef.current;
    xRef.current = Math.min(Math.max(minX, (minX + maxX) * 0.25), maxX);
    setX(xRef.current);
    targetRef.current = xRef.current;

    const tick = () => {
      const speed = busyRef.current ? 42 : 28;
      if (modeRef.current === "walk") {
        const target = targetRef.current;
        const delta = target - xRef.current;
        if (Math.abs(delta) <= 1.2) {
          xRef.current = target;
          setX(xRef.current);
          scheduleIdle();
        } else {
          const step = Math.sign(delta) * Math.min(Math.abs(delta), speed / 60);
          xRef.current += step;
          setFacing(step >= 0 ? 1 : -1);
          setX(xRef.current);
        }
      }
      rafRef.current = window.requestAnimationFrame(tick);
    };

    scheduleIdle();
    rafRef.current = window.requestAnimationFrame(tick);

    const chatStage = stage.closest(".chatStage") ?? stage.parentElement;
    const composer = chatStage?.querySelector(".composer") as HTMLElement | null;
    const ro = new ResizeObserver(() => refreshBounds());
    ro.observe(stage);
    if (composer) ro.observe(composer);
    if (chatStage) ro.observe(chatStage);
    window.addEventListener("resize", refreshBounds);

    return () => {
      clearTimer();
      if (rafRef.current !== null) window.cancelAnimationFrame(rafRef.current);
      ro.disconnect();
      window.removeEventListener("resize", refreshBounds);
    };
  }, []);

  useEffect(() => {
    const stage = stageRef.current;
    if (!busy || !stage) return;
    boundsRef.current = measureWalkBounds(stage);
    const { minX, maxX } = boundsRef.current;
    const next = randomBetween(minX, Math.max(minX, maxX));
    targetRef.current = next;
    modeRef.current = "walk";
    setMode("walk");
    setFacing(next >= xRef.current ? 1 : -1);
  }, [busy]);

  useEffect(() => {
    if (mode !== "walk") {
      setFrame(0);
      return;
    }
    const ms = busy ? 160 : 200;
    const id = window.setInterval(() => {
      setFrame((current) => (current + 1) % WALK_FRAMES);
    }, ms);
    return () => window.clearInterval(id);
  }, [mode, busy]);

  const src =
    mode === "walk"
      ? brandUrl(`/brand/agent-walk-${frame}.webp`)
      : brandUrl("/brand/agent-presence.webp");

  // Sprites face LEFT. facing: 1 = move right, -1 = move left.
  const scaleX = facing >= 0 ? -1 : 1;

  return (
    <aside
      ref={stageRef}
      className={[
        "agentPresence",
        mode === "walk" ? "walking" : "idle",
        busy ? "busy" : "",
      ]
        .filter(Boolean)
        .join(" ")}
      aria-label="EvoHime"
      aria-hidden="true"
    >
      <div
        className="agentPresenceMover"
        style={{
          transform: `translate3d(${x}px, 0, 0) scaleX(${scaleX})`,
        }}
      >
        <img
          className="agentPresenceBody"
          src={src}
          alt=""
          draggable={false}
          decoding="async"
        />
      </div>
    </aside>
  );
}
