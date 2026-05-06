// atomio — right-side panels: debugger, components, simulator, profiler, console.

// ──────────────────  Debugger panel  ──────────────────
function DebuggerPanel({ paused, onResume, onPause, onStep }) {
  const [stackOpen, setStackOpen] = React.useState(true);
  const [varsOpen, setVarsOpen] = React.useState(true);
  const [bpsOpen, setBpsOpen] = React.useState(true);
  const [watchOpen, setWatchOpen] = React.useState(true);

  const valueClass = (type) => {
    if (type === 'number') return 'num';
    if (type === 'boolean') return 'bool';
    if (type === 'function') return 'fn';
    if (type.startsWith('Array') || type === 'Object') return 'obj';
    return '';
  };

  return (
    <div className="panel" style={{ width: 360 }}>
      <div className="debug-controls">
        {paused ? (
          <button className="btn primary" onClick={onResume} title="Continue (F5)"><IcnPlay size={14} /></button>
        ) : (
          <button className="btn" onClick={onPause} title="Pause"><IcnPause size={14} /></button>
        )}
        <button className="btn" onClick={() => onStep('over')} title="Step Over (F10)"><IcnStepOver size={14} /></button>
        <button className="btn" onClick={() => onStep('into')} title="Step Into (F11)"><IcnStepInto size={14} /></button>
        <button className="btn" onClick={() => onStep('out')} title="Step Out (⇧F11)"><IcnStepOut size={14} /></button>
        <button className="btn" title="Restart (⇧⌘F5)"><IcnRestart size={14} /></button>
        <button className="btn" title="Stop (⇧F5)" style={{ color: 'var(--error)' }}><IcnStop size={12} /></button>
        <div className="status">
          <span style={{
            width: 7, height: 7, borderRadius: '50%',
            background: paused ? 'var(--warn)' : 'var(--accent)',
            boxShadow: paused
              ? '0 0 8px rgba(255,180,84,0.6)'
              : '0 0 8px rgba(62,207,142,0.6)',
            animation: 'blink 1.4s infinite',
          }} />
          {paused ? 'Paused' : 'Running'}
        </div>
      </div>

      <div className="panel-section" style={{ flex: 1, overflowY: 'auto' }}>
        {/* Call stack */}
        <div className="panel-section-head" onClick={() => setStackOpen(!stackOpen)}>
          <span className="chev">{stackOpen ? '▾' : '▸'}</span>
          Call Stack
          <span className="count">{MOCK_CALL_STACK.length}</span>
        </div>
        {stackOpen && (
          <div className="panel-section-body" style={{ padding: 0 }}>
            {MOCK_CALL_STACK.map((s, i) => (
              <div key={i} className={'stack-row ' + (s.current ? 'current' : '')}>
                <span className="fn">{s.fn}</span>
                <span className="loc">{s.file.split('/').pop()}:{s.line}</span>
              </div>
            ))}
          </div>
        )}

        {/* Variables */}
        <div className="panel-section-head" onClick={() => setVarsOpen(!varsOpen)} style={{ borderTop: '1px solid var(--line-1)' }}>
          <span className="chev">{varsOpen ? '▾' : '▸'}</span>
          Variables
        </div>
        {varsOpen && (
          <div className="panel-section-body" style={{ padding: 0 }}>
            {MOCK_VARIABLES.map((scope, si) => (
              <div key={si}>
                <div style={{
                  padding: '4px 10px', fontSize: 10, color: 'var(--tx-4)',
                  fontWeight: 600, letterSpacing: '0.06em', textTransform: 'uppercase',
                }}>{scope.scope}</div>
                {scope.items.map((v, i) => (
                  <div key={i} className="var-row">
                    <span className="chev">{v.expandable ? '▸' : ''}</span>
                    <span className="nm">{v.name}</span>
                    <span className="ty">: {v.type}</span>
                    <span className={'vl ' + valueClass(v.type)}>{v.value}</span>
                  </div>
                ))}
              </div>
            ))}
          </div>
        )}

        {/* Watch */}
        <div className="panel-section-head" onClick={() => setWatchOpen(!watchOpen)} style={{ borderTop: '1px solid var(--line-1)' }}>
          <span className="chev">{watchOpen ? '▾' : '▸'}</span>
          Watch
          <span className="count">2</span>
        </div>
        {watchOpen && (
          <div className="panel-section-body" style={{ padding: 0 }}>
            <div className="var-row"><span className="chev">▸</span><span className="nm">items.length</span><span className="ty">: number</span><span className="vl num">3</span></div>
            <div className="var-row"><span className="chev">▸</span><span className="nm">total &gt; 10</span><span className="ty">: boolean</span><span className="vl bool">true</span></div>
            <div className="var-row" style={{ color: 'var(--tx-4)', fontStyle: 'italic' }}>
              <span className="chev">＋</span>
              <span>Add expression…</span>
            </div>
          </div>
        )}

        {/* Breakpoints */}
        <div className="panel-section-head" onClick={() => setBpsOpen(!bpsOpen)} style={{ borderTop: '1px solid var(--line-1)' }}>
          <span className="chev">{bpsOpen ? '▾' : '▸'}</span>
          Breakpoints
          <span className="count">{MOCK_BREAKPOINT_LIST.filter(b => b.enabled).length}/{MOCK_BREAKPOINT_LIST.length}</span>
        </div>
        {bpsOpen && (
          <div className="panel-section-body" style={{ padding: 0 }}>
            {MOCK_BREAKPOINT_LIST.map((bp, i) => (
              <div key={i} className={'bp-row ' + (bp.enabled ? '' : 'off')}>
                <span className="bp-tog" />
                <span className="file">{bp.file}</span>
                <span className="line">:{bp.line}</span>
                <span className="expr">{bp.expr}</span>
                {bp.hits > 0 && <span className="hits">×{bp.hits}</span>}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

// ──────────────────  Console / logs  ──────────────────
function ConsolePanel() {
  const [filter, setFilter] = React.useState('all');
  const [logs, setLogs] = React.useState(MOCK_LOGS);
  const [input, setInput] = React.useState('');
  const ref = React.useRef(null);

  React.useEffect(() => {
    if (ref.current) ref.current.scrollTop = ref.current.scrollHeight;
  }, [logs]);

  // Stream new logs every 4s
  React.useEffect(() => {
    const i = setInterval(() => {
      setLogs(prev => {
        const next = [...prev];
        const now = new Date();
        const t = now.toTimeString().slice(0, 8) + '.' + String(now.getMilliseconds()).padStart(3, '0');
        const samples = [
          { lvl: 'log', src: 'app', msg: '[render] CartScreen reconciled in ' + (3 + Math.random() * 4).toFixed(1) + 'ms' },
          { lvl: 'debug', src: 'metro', msg: 'HMR · components/CartItem.tsx' },
          { lvl: 'log', src: 'app', msg: 'Network: GET /api/products → 200 (' + (40 + Math.random() * 80).toFixed(0) + 'ms)' },
          { lvl: 'info', src: 'expo', msg: 'Fast Refresh applied · 1 module' },
        ];
        next.push({ t, ...samples[Math.floor(Math.random() * samples.length)] });
        if (next.length > 80) next.shift();
        return next;
      });
    }, 4200);
    return () => clearInterval(i);
  }, []);

  const counts = {
    all: logs.length,
    log: logs.filter(l => l.lvl === 'log').length,
    warn: logs.filter(l => l.lvl === 'warn').length,
    error: logs.filter(l => l.lvl === 'error').length,
    debug: logs.filter(l => l.lvl === 'debug').length,
  };
  const filtered = filter === 'all' ? logs : logs.filter(l => l.lvl === filter || (filter === 'log' && l.lvl === 'info'));

  return (
    <div className="console">
      <div className="console-head">
        <div className="filter">
          {[
            ['all', 'All'], ['log', 'Logs'], ['warn', 'Warnings'], ['error', 'Errors'], ['debug', 'Debug'],
          ].map(([k, l]) => (
            <span key={k} className={'f ' + (filter === k ? 'on' : '')} onClick={() => setFilter(k)}>
              {l}<span className="num">{counts[k]}</span>
            </span>
          ))}
        </div>
        <div style={{ flex: 1 }} />
        <span style={{ color: 'var(--tx-4)', fontSize: 10.5, display: 'flex', alignItems: 'center', gap: 5 }}>
          <span style={{ width: 6, height: 6, borderRadius: '50%', background: 'var(--accent)', boxShadow: '0 0 6px rgba(62,207,142,0.6)' }} />
          Streaming · Metro :8081
        </span>
        <span style={{ color: 'var(--tx-3)', cursor: 'pointer', fontSize: 11 }} onClick={() => setLogs([])}>Clear</span>
      </div>
      <div className="console-body" ref={ref}>
        {filtered.map((l, i) => (
          <div key={i} className={'log-row ' + l.lvl}>
            <span className="t">{l.t}</span>
            <span className="lvl">{l.lvl}</span>
            <span className="src">{l.src}</span>
            <span className="msg">{l.msg}</span>
          </div>
        ))}
      </div>
      <div className="console-input">
        <span className="prompt">›</span>
        <input
          placeholder="Evaluate in app context… (⇧⏎ for multiline)"
          value={input}
          onChange={(e) => setInput(e.target.value)}
        />
        <span className="caret" />
      </div>
    </div>
  );
}

// ──────────────────  Simulator panel  ──────────────────
function SimulatorPanel() {
  const [device, setDevice] = React.useState(0);
  const [tab, setTab] = React.useState('cart');
  return (
    <div className="panel" style={{ width: 320, background: 'var(--bg-2)' }}>
      <div className="panel-head">
        <span className="title">iOS Simulator</span>
        <span className="pill" style={{ color: 'var(--accent)' }}>● live</span>
      </div>
      <div style={{ padding: '8px 10px', display: 'flex', gap: 6, alignItems: 'center', borderBottom: '1px solid var(--line-1)' }}>
        <select
          value={device}
          onChange={(e) => setDevice(parseInt(e.target.value))}
          style={{
            background: 'var(--bg-3)', color: 'var(--tx-1)',
            border: '1px solid var(--line-2)', borderRadius: 4,
            padding: '4px 8px', fontSize: 11, fontFamily: 'inherit',
            outline: 'none', flex: 1,
          }}
        >
          {MOCK_DEVICES.map((d, i) => (
            <option key={i} value={i}>{d.name} · {d.os}</option>
          ))}
        </select>
        <button className="btn-ghost" style={{ padding: '4px 8px' }}><IcnReload size={12} /></button>
        <button className="btn-ghost" style={{ padding: '4px 8px' }} title="Inspect element"><IcnInspect size={12} /></button>
      </div>

      <div style={{ flex: 1, display: 'grid', placeItems: 'center', padding: 18, background: 'radial-gradient(circle at 50% 30%, rgba(62,207,142,0.05), transparent 70%)' }}>
        <PhoneMock activeTab={tab} setTab={setTab} />
      </div>

      <div style={{
        padding: '8px 12px', borderTop: '1px solid var(--line-1)',
        display: 'flex', gap: 8, alignItems: 'center', fontSize: 10.5, color: 'var(--tx-3)',
      }}>
        <span className="chip"><span style={{ width: 6, height: 6, borderRadius: '50%', background: 'var(--accent)' }} /> 60fps</span>
        <span className="chip">JS 14ms</span>
        <span className="chip">UI 8ms</span>
        <div style={{ flex: 1 }} />
        <span style={{ color: 'var(--tx-4)' }}>⌘R reload</span>
      </div>
    </div>
  );
}

// Phone mock that renders a fake cart UI matching the code
function PhoneMock({ activeTab, setTab }) {
  return (
    <div style={{
      width: 240, height: 480,
      background: '#0a0c0f',
      borderRadius: 36,
      padding: 6,
      boxShadow: '0 0 0 2px #1a1d23, 0 30px 60px -20px rgba(0,0,0,0.8)',
      position: 'relative',
    }}>
      <div style={{
        background: '#fafafa', borderRadius: 30, height: '100%',
        overflow: 'hidden', display: 'flex', flexDirection: 'column',
        position: 'relative',
      }}>
        {/* Notch */}
        <div style={{
          position: 'absolute', top: 6, left: '50%', transform: 'translateX(-50%)',
          width: 80, height: 22, background: '#0a0c0f', borderRadius: 12, zIndex: 5,
        }} />
        {/* Status */}
        <div style={{
          height: 36, display: 'flex', alignItems: 'flex-end', justifyContent: 'space-between',
          padding: '0 18px 4px', fontSize: 10.5, color: '#0a0c0f', fontWeight: 600,
          fontFamily: '-apple-system, sans-serif',
        }}>
          <span>9:41</span>
          <span style={{ display: 'flex', gap: 4, fontSize: 9 }}>●●●● 100</span>
        </div>
        {/* Header */}
        <div style={{ padding: '14px 18px 8px', display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <span style={{ fontSize: 22, fontWeight: 700, color: '#0a0c0f', letterSpacing: '-0.02em' }}>Your Cart</span>
          <span style={{ width: 28, height: 28, borderRadius: '50%', background: '#eee' }} />
        </div>
        {/* Items */}
        <div style={{ flex: 1, padding: '4px 14px', overflow: 'hidden' }}>
          {[
            { n: 'Flat White', sz: 'Medium · oat', p: '4.50', c: '#7d5a3f' },
            { n: 'Iced Latte', sz: 'Large · whole', p: '5.50', c: '#a07f5c' },
            { n: 'Cortado',    sz: 'Small · oat',  p: '4.50', c: '#5a3e2b' },
          ].map((it, i) => (
            <div key={i} style={{
              display: 'flex', alignItems: 'center', gap: 10,
              padding: '8px 4px', borderBottom: '1px solid #eee',
            }}>
              <div style={{
                width: 36, height: 36, borderRadius: 10,
                background: it.c, opacity: 0.85,
              }} />
              <div style={{ flex: 1 }}>
                <div style={{ fontSize: 12, fontWeight: 600, color: '#0a0c0f' }}>{it.n}</div>
                <div style={{ fontSize: 9.5, color: '#888' }}>{it.sz}</div>
              </div>
              <div style={{ fontSize: 12, fontWeight: 600, color: '#0a0c0f' }}>${it.p}</div>
            </div>
          ))}
        </div>
        {/* Checkout button */}
        <div style={{ padding: '10px 14px 14px' }}>
          <div style={{
            background: '#0a0c0f', color: '#fff',
            padding: '12px', borderRadius: 14, textAlign: 'center',
            fontSize: 13, fontWeight: 600,
            display: 'flex', justifyContent: 'space-between', alignItems: 'center',
            padding: '12px 18px',
          }}>
            <span>Checkout</span><span>$14.50</span>
          </div>
        </div>
        {/* Tab bar */}
        <div style={{
          display: 'flex', justifyContent: 'space-around',
          padding: '8px 0 14px', background: '#fff', borderTop: '1px solid #eee',
        }}>
          {['shop', 'cart', 'profile'].map((t) => (
            <div
              key={t}
              onClick={() => setTab(t)}
              style={{
                fontSize: 9, fontWeight: 600,
                color: activeTab === t ? '#0a0c0f' : '#bbb',
                textTransform: 'capitalize', cursor: 'pointer',
              }}
            >
              <div style={{
                width: 18, height: 18, margin: '0 auto 2px',
                borderRadius: 4,
                background: activeTab === t ? '#0a0c0f' : '#ccc',
              }} />
              {t}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

// ──────────────────  Component tree  ──────────────────
function ComponentRow({ node, expanded, setExpanded }) {
  const isOpen = expanded[node.name + node.depth] !== false;
  const hasKids = node.children && node.children.length > 0;
  return (
    <>
      <div
        className={'ct-row ' + (node.selected ? 'selected' : '')}
        style={{ paddingLeft: 8 + node.depth * 14 }}
        onClick={() => setExpanded({ ...expanded, [node.name + node.depth]: !isOpen })}
      >
        <span className="chev">
          {hasKids ? (isOpen ? '▾' : '▸') : ''}
        </span>
        <span style={{ color: 'var(--tx-4)' }}>&lt;</span>
        <span className={'nm ' + node.kind}>{node.name}</span>
        <span style={{ color: 'var(--tx-4)' }}>&gt;</span>
        {node.props && <span className="props">{node.props}</span>}
      </div>
      {isOpen && hasKids && node.children.map((c, i) => (
        <ComponentRow key={i} node={c} expanded={expanded} setExpanded={setExpanded} />
      ))}
    </>
  );
}

function ComponentTreePanel() {
  const [expanded, setExpanded] = React.useState({});
  return (
    <div className="panel" style={{ width: 380 }}>
      <div className="panel-head">
        <span className="title">Components</span>
        <span className="pill">React 18</span>
      </div>
      <div style={{ flex: 1, overflowY: 'auto', minHeight: 0 }}>
        <div style={{ padding: '6px 0', borderBottom: '1px solid var(--line-1)' }}>
          {MOCK_COMPONENT_TREE.map((n, i) => (
            <ComponentRow key={i} node={n} expanded={expanded} setExpanded={setExpanded} />
          ))}
        </div>
        <div style={{ padding: '10px 12px' }}>
          <div style={{
            fontSize: 10, color: 'var(--tx-4)', fontWeight: 600,
            letterSpacing: '0.08em', textTransform: 'uppercase', marginBottom: 8,
          }}>Selected · {MOCK_SELECTED_COMPONENT.name}</div>
          <div style={{ fontSize: 11, color: 'var(--tx-3)', fontFamily: 'var(--font-mono)', marginBottom: 12 }}>
            {MOCK_SELECTED_COMPONENT.source}
          </div>
          <div style={{
            fontSize: 10, color: 'var(--tx-4)', fontWeight: 600,
            letterSpacing: '0.08em', textTransform: 'uppercase', marginBottom: 6,
          }}>Hooks</div>
          {MOCK_SELECTED_COMPONENT.hooks.map((h, i) => (
            <div key={i} style={{
              padding: '6px 8px', marginBottom: 4,
              background: 'var(--bg-3)', borderRadius: 5,
              fontFamily: 'var(--font-mono)', fontSize: 11,
            }}>
              <span style={{ color: 'var(--info)' }}>{h.name}</span>
              <span style={{ color: 'var(--tx-4)' }}> → </span>
              <span style={{ color: 'var(--sx-str)' }}>{h.value}</span>
            </div>
          ))}
          <div style={{
            marginTop: 14, padding: '8px 10px',
            background: 'var(--accent-soft)', border: '1px solid var(--accent-line)',
            borderRadius: 6, fontSize: 11, color: 'var(--tx-2)',
            display: 'flex', gap: 8, alignItems: 'center',
          }}>
            <IcnZap size={13} /><span>{MOCK_SELECTED_COMPONENT.rendered}</span>
          </div>
        </div>
      </div>
    </div>
  );
}

// ──────────────────  Profiler  ──────────────────
function ProfilerPanel() {
  return (
    <div className="panel" style={{ width: 460 }}>
      <div className="panel-head">
        <span className="title">Profiler</span>
        <span className="pill" style={{ color: 'var(--warn)' }}>2 dropped frames</span>
      </div>
      <div style={{ flex: 1, overflowY: 'auto', minHeight: 0, padding: 14 }}>

        {/* FPS chart */}
        <div style={{
          fontSize: 10, color: 'var(--tx-4)', fontWeight: 600,
          letterSpacing: '0.08em', textTransform: 'uppercase', marginBottom: 8,
        }}>JS Frame Time · 60fps target</div>
        <div style={{
          display: 'flex', alignItems: 'flex-end', gap: 1,
          height: 80, padding: '4px 0',
          background: 'var(--bg-1)', borderRadius: 6,
          position: 'relative',
          border: '1px solid var(--line-1)',
          paddingLeft: 8, paddingRight: 8,
        }}>
          {/* 16ms target line */}
          <div style={{
            position: 'absolute', left: 8, right: 8,
            bottom: 4 + (16 / 50 * 76),
            borderTop: '1px dashed var(--line-3)',
            opacity: 0.6,
          }} />
          {MOCK_FRAMES.map((v, i) => {
            const h = (v / 50) * 76;
            const color = v <= 16 ? 'var(--accent)' : v <= 33 ? 'var(--warn)' : 'var(--error)';
            return (
              <div key={i} style={{
                width: 6, height: h, background: color,
                borderRadius: '1px 1px 0 0', opacity: 0.85,
              }} />
            );
          })}
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 4, fontSize: 10, color: 'var(--tx-4)', fontFamily: 'var(--font-mono)' }}>
          <span>0ms</span><span>16ms ─ ─ ─</span><span>50ms</span>
        </div>

        {/* Stats */}
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 8, marginTop: 18 }}>
          {[
            { label: 'JS · avg', v: '12.4ms', good: true },
            { label: 'UI · avg', v: '8.1ms', good: true },
            { label: 'p95', v: '28ms', good: false },
            { label: 'Renders', v: '142', good: true },
          ].map((s, i) => (
            <div key={i} style={{
              padding: 10, background: 'var(--bg-1)',
              border: '1px solid var(--line-1)', borderRadius: 6,
            }}>
              <div style={{ fontSize: 9.5, color: 'var(--tx-4)', textTransform: 'uppercase', letterSpacing: '0.08em', fontWeight: 600 }}>{s.label}</div>
              <div style={{
                fontSize: 18, fontWeight: 600, marginTop: 4,
                color: s.good ? 'var(--accent)' : 'var(--warn)',
                fontFamily: 'var(--font-mono)', letterSpacing: '-0.02em',
              }}>{s.v}</div>
            </div>
          ))}
        </div>

        {/* Flame graph */}
        <div style={{
          fontSize: 10, color: 'var(--tx-4)', fontWeight: 600,
          letterSpacing: '0.08em', textTransform: 'uppercase', marginTop: 22, marginBottom: 8,
        }}>Flame Graph · last commit</div>
        <div style={{
          background: 'var(--bg-1)', border: '1px solid var(--line-1)',
          borderRadius: 6, padding: 8, position: 'relative', minHeight: 130,
        }}>
          {MOCK_FLAMEGRAPH.map((b, i) => {
            const xPos = b.x === undefined ? 0 : b.x;
            return (
              <div key={i} style={{
                position: 'absolute',
                left: 8 + xPos * 0.55,
                top: 8 + b.d * 22,
                height: 18,
                width: b.w * 0.55,
                background: b.hot ? 'rgba(255,107,107,0.25)' : 'rgba(108,179,255,0.18)',
                border: '1px solid ' + (b.hot ? 'var(--error)' : 'var(--info)'),
                borderRadius: 3,
                fontSize: 10, fontFamily: 'var(--font-mono)',
                color: b.hot ? 'var(--error)' : 'var(--info)',
                padding: '0 6px',
                lineHeight: '16px',
                whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis',
              }}>{b.label}</div>
            );
          })}
        </div>

        <div style={{
          marginTop: 18, padding: 10,
          background: 'var(--warn-soft)',
          border: '1px solid rgba(255,180,84,0.3)',
          borderRadius: 6, fontSize: 11.5, color: 'var(--tx-2)', lineHeight: 1.45,
        }}>
          <div style={{ color: 'var(--warn)', fontWeight: 600, marginBottom: 4, fontSize: 11 }}>⚠ Suggestion</div>
          <code style={{ fontFamily: 'var(--font-mono)' }}>CartScreen</code> re-rendered 14× in 2s. Consider memoizing the FlatList <code style={{ fontFamily: 'var(--font-mono)' }}>renderItem</code> callback.
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { DebuggerPanel, ConsolePanel, SimulatorPanel, ComponentTreePanel, ProfilerPanel });
