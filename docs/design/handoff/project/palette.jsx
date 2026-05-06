// atomio — Command palette and project picker (launch screen).

// ─────────  Command palette  ─────────
function CommandPalette({ onClose }) {
  const [q, setQ] = React.useState('');
  const [sel, setSel] = React.useState(0);
  const inputRef = React.useRef(null);

  React.useEffect(() => {
    inputRef.current?.focus();
    const onKey = (e) => {
      if (e.key === 'Escape') onClose();
      if (e.key === 'ArrowDown') { setSel(s => Math.min(s + 1, results.length - 1)); e.preventDefault(); }
      if (e.key === 'ArrowUp') { setSel(s => Math.max(s - 1, 0)); e.preventDefault(); }
      if (e.key === 'Enter') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  const results = MOCK_COMMANDS.filter(c =>
    !q || c.label.toLowerCase().includes(q.toLowerCase()) || c.cat.toLowerCase().includes(q.toLowerCase())
  );

  // Group by category
  const byCat = {};
  results.forEach(r => { (byCat[r.cat] = byCat[r.cat] || []).push(r); });

  return (
    <div
      onClick={onClose}
      style={{
        position: 'absolute', inset: 0,
        background: 'rgba(5,6,8,0.5)',
        backdropFilter: 'blur(6px)',
        zIndex: 100,
        display: 'flex', justifyContent: 'center', alignItems: 'flex-start',
        paddingTop: '10vh',
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 580, maxWidth: '90%',
          background: 'var(--bg-2)',
          border: '1px solid var(--line-3)',
          borderRadius: 10,
          boxShadow: '0 30px 80px -20px rgba(0,0,0,0.7), 0 0 0 1px rgba(62,207,142,0.08), 0 60px 120px rgba(0,0,0,0.4)',
          overflow: 'hidden',
          fontFamily: 'var(--font-ui)',
        }}
      >
        <div style={{
          display: 'flex', alignItems: 'center', gap: 10,
          padding: '14px 18px', borderBottom: '1px solid var(--line-1)',
        }}>
          <BrandMark size={18} />
          <input
            ref={inputRef}
            value={q}
            onChange={(e) => { setQ(e.target.value); setSel(0); }}
            placeholder="Type a command, or > for actions, : for files…"
            style={{
              flex: 1, background: 'transparent', border: 0, outline: 0,
              color: 'var(--tx-1)', fontSize: 15, fontFamily: 'inherit',
            }}
          />
          <span style={{
            fontSize: 10, color: 'var(--tx-4)',
            fontFamily: 'var(--font-mono)',
            padding: '2px 6px', border: '1px solid var(--line-2)', borderRadius: 3,
          }}>esc</span>
        </div>
        <div style={{ maxHeight: 380, overflowY: 'auto', padding: '6px 0' }}>
          {Object.entries(byCat).map(([cat, items]) => (
            <div key={cat}>
              <div style={{
                padding: '6px 16px', fontSize: 9.5,
                color: 'var(--tx-4)', fontWeight: 700,
                textTransform: 'uppercase', letterSpacing: '0.1em',
              }}>{cat}</div>
              {items.map((c) => {
                const idx = results.indexOf(c);
                const isSel = idx === sel;
                return (
                  <div
                    key={c.id}
                    onMouseEnter={() => setSel(idx)}
                    onClick={onClose}
                    style={{
                      display: 'flex', alignItems: 'center', gap: 10,
                      padding: '8px 16px',
                      background: isSel ? 'var(--accent-soft)' : 'transparent',
                      borderLeft: isSel ? '2px solid var(--accent)' : '2px solid transparent',
                      cursor: 'pointer',
                    }}
                  >
                    <span style={{
                      color: isSel ? 'var(--accent)' : 'var(--tx-3)',
                      fontSize: 13, flex: 1,
                    }}>{c.label}</span>
                    {c.shortcut && (
                      <span style={{
                        fontFamily: 'var(--font-mono)', fontSize: 10.5,
                        color: 'var(--tx-4)',
                        padding: '1px 6px', border: '1px solid var(--line-2)',
                        borderRadius: 3,
                      }}>{c.shortcut}</span>
                    )}
                  </div>
                );
              })}
            </div>
          ))}
          {results.length === 0 && (
            <div style={{ padding: 24, textAlign: 'center', color: 'var(--tx-4)', fontSize: 13 }}>
              No matches.
            </div>
          )}
        </div>
        <div style={{
          padding: '8px 16px', borderTop: '1px solid var(--line-1)',
          background: 'var(--bg-1)',
          display: 'flex', gap: 14, fontSize: 10.5, color: 'var(--tx-4)',
          fontFamily: 'var(--font-mono)',
        }}>
          <span><span style={{ color: 'var(--tx-2)' }}>↑↓</span> navigate</span>
          <span><span style={{ color: 'var(--tx-2)' }}>⏎</span> run</span>
          <span><span style={{ color: 'var(--tx-2)' }}>⇥</span> jump to category</span>
          <span style={{ marginLeft: 'auto' }}>{results.length} results</span>
        </div>
      </div>
    </div>
  );
}

// ─────────  Project picker (launch screen)  ─────────
function ProjectPicker({ onOpen }) {
  return (
    <div style={{
      position: 'absolute', inset: 0,
      background: 'var(--bg-1)',
      display: 'flex', overflow: 'hidden',
    }}>
      {/* Left side — brand */}
      <div style={{
        width: 380, padding: '60px 40px',
        borderRight: '1px solid var(--line-1)',
        display: 'flex', flexDirection: 'column',
        background: 'linear-gradient(180deg, rgba(62,207,142,0.06) 0%, transparent 60%)',
        flexShrink: 0,
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <BrandMark size={32} />
          <div style={{ fontSize: 22, fontWeight: 700, letterSpacing: '-0.02em' }}>atomio</div>
        </div>
        <div style={{
          marginTop: 6, fontSize: 11, color: 'var(--tx-4)',
          fontFamily: 'var(--font-mono)', letterSpacing: '0.05em',
        }}>v 0.4.2 · expo + react native debugger</div>

        <div style={{ marginTop: 60 }}>
          <div style={{
            fontSize: 32, fontWeight: 600, letterSpacing: '-0.025em',
            lineHeight: 1.15, color: 'var(--tx-1)',
          }}>
            Welcome back.<br/>
            <span style={{ color: 'var(--tx-3)' }}>What are we shipping?</span>
          </div>
        </div>

        <div style={{ marginTop: 36, display: 'flex', flexDirection: 'column', gap: 10 }}>
          <button className="btn-primary" onClick={onOpen}>
            <IcnPlus size={13} /> New Expo project
          </button>
          <button className="btn-ghost" onClick={onOpen}>
            <IcnFiles size={13} /> Open from disk…
          </button>
          <button className="btn-ghost" onClick={onOpen}>
            <IcnGit size={13} /> Clone from git…
          </button>
        </div>

        <div style={{ flex: 1 }} />
        <div style={{
          fontSize: 10.5, color: 'var(--tx-5)', fontFamily: 'var(--font-mono)',
          lineHeight: 1.6,
        }}>
          ⌘P  open file<br/>
          ⌘⇧P  command palette<br/>
          F5   start debugging
        </div>
      </div>

      {/* Right side — recents */}
      <div style={{ flex: 1, padding: '60px 50px 40px', overflowY: 'auto', minWidth: 0 }}>
        <div style={{
          fontSize: 11, color: 'var(--tx-4)', fontWeight: 700,
          letterSpacing: '0.1em', textTransform: 'uppercase',
          marginBottom: 22,
        }}>Recent</div>

        <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
          {MOCK_RECENT_PROJECTS.map((p, i) => (
            <div
              key={i}
              onClick={onOpen}
              style={{
                display: 'flex', alignItems: 'center', gap: 16,
                padding: '14px 18px',
                borderRadius: 8,
                cursor: 'pointer',
                transition: 'background 80ms',
                position: 'relative',
              }}
              onMouseEnter={(e) => e.currentTarget.style.background = 'var(--bg-2)'}
              onMouseLeave={(e) => e.currentTarget.style.background = 'transparent'}
            >
              <div style={{
                width: 40, height: 40, borderRadius: 9,
                background: p.accent + '22',
                border: '1px solid ' + p.accent + '55',
                display: 'grid', placeItems: 'center',
                color: p.accent, fontSize: 16, fontWeight: 700,
                letterSpacing: '-0.02em',
                flexShrink: 0,
              }}>{p.name[0].toUpperCase()}</div>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 14.5, fontWeight: 600, color: 'var(--tx-1)', letterSpacing: '-0.005em' }}>{p.name}</div>
                <div style={{ fontSize: 11.5, color: 'var(--tx-4)', fontFamily: 'var(--font-mono)', marginTop: 2 }}>
                  {p.path}
                </div>
              </div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 14, flexShrink: 0 }}>
                <span className="chip" style={{ background: 'transparent', border: '1px solid var(--line-2)' }}>
                  <IcnGit size={10} /> {p.branch}
                </span>
                <span style={{ fontSize: 11, color: 'var(--tx-4)', minWidth: 76, textAlign: 'right' }}>{p.last}</span>
              </div>
            </div>
          ))}
        </div>

        {/* Status hint */}
        <div style={{ marginTop: 50, padding: '20px 22px',
          background: 'var(--bg-2)', borderRadius: 8,
          border: '1px solid var(--line-1)',
          display: 'flex', alignItems: 'center', gap: 14,
        }}>
          <div style={{
            width: 36, height: 36, borderRadius: 9,
            background: 'var(--accent-soft)', border: '1px solid var(--accent-line)',
            display: 'grid', placeItems: 'center', color: 'var(--accent)',
          }}><IcnZap size={16}/></div>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 12.5, fontWeight: 600, color: 'var(--tx-1)' }}>iOS Simulator ready</div>
            <div style={{ fontSize: 11, color: 'var(--tx-4)' }}>iPhone 15 Pro · iOS 17.4 · last booted 2m ago</div>
          </div>
          <button className="btn-ghost" onClick={onOpen}>Boot</button>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { CommandPalette, ProjectPicker });
