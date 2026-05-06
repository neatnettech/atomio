// atomio — main App: shell, layout, navigation, tweaks.

const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "accent": "#3ecf8e",
  "density": "comfortable",
  "rightPanel": "debug",
  "showSimulator": true,
  "fontScale": 1
}/*EDITMODE-END*/;

function App() {
  const [tweaks, setTweak] = useTweaks(TWEAK_DEFAULTS);
  const [route, setRoute] = React.useState('app'); // 'launch' | 'app'
  const [view, setView] = React.useState('files'); // activity bar
  const [activeTab, setActiveTab] = React.useState('app/(tabs)/cart.tsx');
  const [tabs] = React.useState(MOCK_OPEN_TABS);
  const [paused, setPaused] = React.useState(true);
  const [breakpoints, setBreakpoints] = React.useState(MOCK_BREAKPOINTS);
  const [paletteOpen, setPaletteOpen] = React.useState(false);
  const [activeLine, setActiveLine] = React.useState(MOCK_ACTIVE_LINE);
  const [showSim, setShowSim] = React.useState(true);
  const [rightPanel, setRightPanel] = React.useState('debug'); // 'debug' | 'tree' | 'perf' | 'none'

  // Sync tweaks → state
  React.useEffect(() => {
    if (tweaks.rightPanel) setRightPanel(tweaks.rightPanel);
    if (tweaks.showSimulator !== undefined) setShowSim(tweaks.showSimulator);
    if (tweaks.accent) document.documentElement.style.setProperty('--accent', tweaks.accent);
  }, [tweaks]);

  // Map activity bar → right panel
  React.useEffect(() => {
    if (view === 'debug') setRightPanel('debug');
    else if (view === 'tree') setRightPanel('tree');
    else if (view === 'perf') setRightPanel('perf');
    else if (view === 'sim') setShowSim(true);
    else if (view === 'cmd') { setPaletteOpen(true); setView('files'); }
  }, [view]);

  // Keyboard
  React.useEffect(() => {
    const onKey = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'p') {
        e.preventDefault(); setPaletteOpen(true);
      }
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault(); setPaletteOpen(true);
      }
      if (e.key === 'F5') {
        e.preventDefault(); setPaused(p => !p);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const toggleBreakpoint = (ln) => {
    setBreakpoints(prev => prev.includes(ln) ? prev.filter(x => x !== ln) : [...prev, ln]);
  };

  if (route === 'launch') {
    return (
      <div className="desktop-bg">
        <div className="win" data-screen-label="01 Launch" style={{
          width: 1280, height: 800,
        }}>
          <Titlebar simple />
          <ProjectPicker onOpen={() => setRoute('app')} />
        </div>
        <FloatingNav route={route} setRoute={setRoute} />
      </div>
    );
  }

  return (
    <div className="desktop-bg">
      <div className="win" data-screen-label="02 Editor + Debug" style={{
        width: 1440, height: 880,
        display: 'flex', flexDirection: 'column',
        '--accent': tweaks.accent,
      }}>
        <Titlebar
          title={MOCK_PROJECT.name}
          branch={MOCK_PROJECT.branch}
          paused={paused}
        />
        <div className="split h" style={{ flex: 1, minHeight: 0 }}>
          <ActivityBar view={view} setView={setView} />
          {/* Sidebar: file tree */}
          <div style={{
            width: 240, background: 'var(--bg-2)',
            borderRight: '1px solid var(--line-1)',
            display: 'flex', flexDirection: 'column', flexShrink: 0,
          }}>
            {view === 'search' ? <SearchSidebar /> : <FileTree />}
          </div>

          {/* Main editor area */}
          <div className="split v" style={{ flex: 1, minWidth: 0 }}>
            <TabBar tabs={tabs} activeTab={activeTab} setActiveTab={setActiveTab} />
            <div className="split h" style={{ flex: 1, minHeight: 0 }}>
              <div className="split v" style={{ flex: 1, minWidth: 0 }}>
                <CodeView
                  paused={paused}
                  breakpoints={breakpoints}
                  activeLine={activeLine}
                  onToggleBreakpoint={toggleBreakpoint}
                />
                <div className="gripper-v" />
                <div style={{ height: 240, display: 'flex', flexDirection: 'column' }}>
                  <ConsolePanel />
                </div>
              </div>
              {showSim && (
                <>
                  <div className="gripper-h" />
                  <SimulatorPanel />
                </>
              )}
            </div>
          </div>

          {/* Right docked panel */}
          {rightPanel !== 'none' && (
            <>
              <div className="gripper-h" />
              {rightPanel === 'debug' && (
                <DebuggerPanel
                  paused={paused}
                  onResume={() => setPaused(false)}
                  onPause={() => setPaused(true)}
                  onStep={() => setActiveLine(l => l + 1)}
                />
              )}
              {rightPanel === 'tree' && <ComponentTreePanel />}
              {rightPanel === 'perf' && <ProfilerPanel />}
            </>
          )}
        </div>
        <StatusBar paused={paused} breakpoints={breakpoints} />
      </div>

      {paletteOpen && <CommandPalette onClose={() => setPaletteOpen(false)} />}
      <FloatingNav route={route} setRoute={setRoute} setPaletteOpen={setPaletteOpen} />

      <TweaksPanel>
        <TweakSection label="Theme" />
        <TweakColor
          label="Accent"
          value={tweaks.accent}
          onChange={(v) => setTweak('accent', v)}
          options={['#3ecf8e', '#7c5cff', '#ff8a3d', '#4aa3ff', '#ff5c8a']}
        />
        <TweakSection label="Layout" />
        <TweakRadio
          label="Right panel"
          value={rightPanel}
          onChange={(v) => { setRightPanel(v); setTweak('rightPanel', v); }}
          options={['debug', 'tree', 'perf']}
        />
        <TweakToggle
          label="Show simulator"
          value={showSim}
          onChange={(v) => { setShowSim(v); setTweak('showSimulator', v); }}
        />
        <TweakSection label="Navigation" />
        <TweakButton label="Show launch screen" onClick={() => setRoute('launch')} />
        <TweakButton label="Show editor" onClick={() => setRoute('app')} />
        <TweakButton label="Open command palette" onClick={() => setPaletteOpen(true)} />
        <TweakSection label="Debug state" />
        <TweakToggle
          label="Paused at breakpoint"
          value={paused}
          onChange={setPaused}
        />
      </TweaksPanel>
    </div>
  );
}

// ─────────  Title bar  ─────────
function Titlebar({ title, branch, paused, simple = false }) {
  return (
    <div className="titlebar">
      <div className="lights">
        <span className="l r" /><span className="l y" /><span className="l g" />
      </div>
      {!simple && (
        <>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginLeft: 12 }}>
            <BrandMark size={14} />
            <span style={{ fontSize: 11.5, color: 'var(--tx-2)', fontWeight: 600 }}>atomio</span>
          </div>
          <div style={{ width: 1, height: 14, background: 'var(--line-2)' }} />
        </>
      )}
      <div className="doc-title">
        {simple ? 'atomio' : title}
        {branch && <span className="branch">⌥ {branch}</span>}
      </div>
      {!simple && (
        <div style={{ display: 'flex', gap: 6, alignItems: 'center', WebkitAppRegion: 'no-drag' }}>
          <span className="chip" style={{ color: paused ? 'var(--warn)' : 'var(--accent)' }}>
            <span style={{
              width: 6, height: 6, borderRadius: '50%',
              background: paused ? 'var(--warn)' : 'var(--accent)',
              boxShadow: '0 0 6px currentColor',
            }} />
            {paused ? 'Paused' : 'Running'}
          </span>
          <span className="chip"><IcnDevice size={11} /> iPhone 15 Pro</span>
          <span style={{ width: 8 }} />
        </div>
      )}
    </div>
  );
}

// ─────────  Status bar  ─────────
function StatusBar({ paused, breakpoints }) {
  return (
    <div className="statusbar">
      <span className="seg brand">
        <span className="dot" />
        Metro :8081
      </span>
      <span className="seg"><IcnGit size={11} /> feat/cart-checkout · ↑2 ↓0</span>
      <span className="seg err"><span className="dot err" /> 1 error</span>
      <span className="seg warn"><span className="dot warn" /> 2 warnings</span>
      <span className="spacer" />
      <span className="seg">Ln 12, Col 28</span>
      <span className="seg">Spaces: 2</span>
      <span className="seg">UTF-8</span>
      <span className="seg">TypeScript React</span>
      <span className="seg">{breakpoints.length} bp</span>
      <span className="seg">{paused ? '⏸︎ paused' : '▶ live'}</span>
    </div>
  );
}

// ─────────  Search sidebar  ─────────
function SearchSidebar() {
  return (
    <div style={{ flex: 1, padding: '10px 12px', display: 'flex', flexDirection: 'column', gap: 8 }}>
      <div style={{
        display: 'flex', alignItems: 'center', gap: 6,
        padding: '6px 10px', background: 'var(--bg-3)',
        borderRadius: 5, border: '1px solid var(--line-2)',
      }}>
        <IcnSearch size={12} />
        <input placeholder="Search project…" style={{
          flex: 1, background: 'transparent', border: 0, outline: 0,
          color: 'var(--tx-1)', fontSize: 12, fontFamily: 'var(--font-mono)',
        }} defaultValue="useCart" />
      </div>
      <div style={{
        display: 'flex', alignItems: 'center', gap: 6,
        padding: '6px 10px', background: 'var(--bg-3)',
        borderRadius: 5, border: '1px solid var(--line-2)',
      }}>
        <span style={{ color: 'var(--tx-4)', fontSize: 11 }}>↳</span>
        <input placeholder="Replace…" style={{
          flex: 1, background: 'transparent', border: 0, outline: 0,
          color: 'var(--tx-1)', fontSize: 12, fontFamily: 'var(--font-mono)',
        }} />
      </div>

      <div style={{ display: 'flex', gap: 4, fontSize: 10.5, color: 'var(--tx-4)', marginTop: 2 }}>
        <span style={{ padding: '2px 6px', border: '1px solid var(--line-2)', borderRadius: 3, color: 'var(--tx-2)' }}>Aa</span>
        <span style={{ padding: '2px 6px', border: '1px solid var(--line-2)', borderRadius: 3 }}>W</span>
        <span style={{ padding: '2px 6px', border: '1px solid var(--line-2)', borderRadius: 3, background: 'var(--bg-3)', color: 'var(--accent)' }}>.*</span>
        <div style={{ flex: 1 }} />
        <span style={{ padding: '2px 6px', color: 'var(--tx-3)' }}>4 in 2 files</span>
      </div>

      <div style={{ marginTop: 8, fontFamily: 'var(--font-mono)', fontSize: 11.5 }}>
        <div style={{ padding: '6px 4px', display: 'flex', gap: 6, alignItems: 'center', color: 'var(--tx-2)' }}>
          <span style={{ color: 'var(--tx-4)' }}>▾</span>
          <FileIcon lang="tsx" />
          cart.tsx <span style={{ color: 'var(--tx-4)' }}>· 3</span>
        </div>
        {[
          { ln: 3,  before: 'import { ', match: 'useCart', after: " } from '@/hooks/useCart'" },
          { ln: 8,  before: '  const { items, total, remove, checkout } = ', match: 'useCart', after: '()' },
        ].map((r, i) => (
          <div key={i} style={{ padding: '2px 8px 2px 28px', display: 'flex', gap: 8 }}>
            <span style={{ color: 'var(--tx-5)', minWidth: 22, textAlign: 'right' }}>{r.ln}</span>
            <span style={{ color: 'var(--tx-3)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
              {r.before}
              <span style={{ background: 'var(--accent-soft)', color: 'var(--accent)', padding: '0 2px', borderRadius: 2 }}>{r.match}</span>
              {r.after}
            </span>
          </div>
        ))}
        <div style={{ padding: '6px 4px', display: 'flex', gap: 6, alignItems: 'center', color: 'var(--tx-2)', marginTop: 6 }}>
          <span style={{ color: 'var(--tx-4)' }}>▾</span>
          <FileIcon lang="ts" />
          useCart.ts <span style={{ color: 'var(--tx-4)' }}>· 1</span>
        </div>
        <div style={{ padding: '2px 8px 2px 28px', display: 'flex', gap: 8 }}>
          <span style={{ color: 'var(--tx-5)', minWidth: 22, textAlign: 'right' }}>14</span>
          <span style={{ color: 'var(--tx-3)' }}>
            export function <span style={{ background: 'var(--accent-soft)', color: 'var(--accent)', padding: '0 2px', borderRadius: 2 }}>useCart</span>() {'{'}
          </span>
        </div>
      </div>
    </div>
  );
}

// ─────────  Floating top-bar nav (so user can jump to launch screen)  ─────────
function FloatingNav({ route, setRoute, setPaletteOpen }) {
  return (
    <div style={{
      position: 'fixed', top: 18, right: 18, zIndex: 50,
      display: 'flex', gap: 6,
      background: 'rgba(20,24,31,0.85)',
      backdropFilter: 'blur(12px)',
      WebkitBackdropFilter: 'blur(12px)',
      border: '1px solid var(--line-2)',
      borderRadius: 999, padding: 4,
      boxShadow: '0 10px 30px rgba(0,0,0,0.35)',
    }}>
      <button
        onClick={() => setRoute('launch')}
        style={navBtnStyle(route === 'launch')}
      >Launch</button>
      <button
        onClick={() => setRoute('app')}
        style={navBtnStyle(route === 'app')}
      >Editor</button>
      {setPaletteOpen && (
        <button
          onClick={() => setPaletteOpen(true)}
          style={navBtnStyle(false)}
        >⌘K Palette</button>
      )}
    </div>
  );
}
const navBtnStyle = (on) => ({
  padding: '6px 12px',
  fontSize: 11, fontWeight: 600,
  background: on ? 'var(--accent-soft)' : 'transparent',
  color: on ? 'var(--accent)' : 'var(--tx-2)',
  border: 0, borderRadius: 999,
  cursor: 'pointer', fontFamily: 'inherit',
});

// Mount
const root = ReactDOM.createRoot(document.getElementById('root'));
root.render(<App />);
