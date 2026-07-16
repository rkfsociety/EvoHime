import { useEffect, useRef, useState } from "react";

type AgentPresenceProps = {
  busy?: boolean;
};

type WalkMode = "idle" | "walk";

const BODY_WIDTH = 150;
const EDGE_PAD = 8;
const WALK_FRAMES = 6;

function randomBetween(min: number, max: number) {
  return min + Math.random() * (max - min);
}

export function AgentPresence({ busy = false }: AgentPresenceProps) {
  const stageRef = useRef<HTMLElement | null>(null);
  const xRef = useRef(24);
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
    const stage = stageRef.current;
    if (!stage) return;

    const maxX = () => Math.max(EDGE_PAD, stage.clientWidth - BODY_WIDTH - EDGE_PAD);

    const clearTimer = () => {
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };

    const startWalkTo = (next: number) => {
      targetRef.current = next;
      modeRef.current = "walk";
      setMode("walk");
      setFacing(next >= xRef.current ? 1 : -1);
    };

    const scheduleIdle = () => {
      clearTimer();
      modeRef.current = "idle";
      setMode("idle");
      setFrame(0);
      const pause = busyRef.current ? randomBetween(350, 800) : randomBetween(1200, 3600);
      timerRef.current = window.setTimeout(() => {
        startWalkTo(randomBetween(EDGE_PAD, maxX()));
      }, pause);
    };

    xRef.current = Math.min(Math.max(EDGE_PAD, stage.clientWidth * 0.12), maxX());
    setX(xRef.current);
    targetRef.current = xRef.current;

    const tick = () => {
      const speed = busyRef.current ? 90 : 52;
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

    const onResize = () => {
      const capped = Math.min(xRef.current, maxX());
      xRef.current = capped;
      setX(capped);
      targetRef.current = Math.min(targetRef.current, maxX());
    };
    const ro = new ResizeObserver(onResize);
    ro.observe(stage);

    return () => {
      clearTimer();
      if (rafRef.current !== null) window.cancelAnimationFrame(rafRef.current);
      ro.disconnect();
    };
  }, []);

  useEffect(() => {
    const stage = stageRef.current;
    if (!busy || !stage) return;
    const maxX = Math.max(EDGE_PAD, stage.clientWidth - BODY_WIDTH - EDGE_PAD);
    const next = randomBetween(EDGE_PAD, maxX);
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
    const ms = busy ? 90 : 120;
    const id = window.setInterval(() => {
      setFrame((current) => (current + 1) % WALK_FRAMES);
    }, ms);
    return () => window.clearInterval(id);
  }, [mode, busy]);

  const src =
    mode === "walk"
      ? `/brand/agent-walk-${frame}.webp`
      : "/brand/agent-idle.webp";

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
          transform: `translate3d(${x}px, 0, 0) scaleX(${facing})`,
        }}
      >
        <img
          className="agentPresenceBody"
          src={src}
          alt=""
          draggable={false}
        />
      </div>
    </aside>
  );
}
