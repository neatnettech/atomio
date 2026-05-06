// atomio — Editor surface: file tree, tabs, code with debug overlays.

// ─────────────  Activity bar (left rail)  ─────────────
function ActivityBar({ view, setView }) {
  const items = [
    { id: 'files',   icon: <IcnFiles size={18} />,    label: 'Files' },
    { id: 'search',  icon: <IcnSearch size={18} />,   label: 'Search' },
    { id: 'debug',   icon: <IcnDebug size={18} />,    label: 'Debug', badge: 3 },
    { id: 'tree',    icon: <IcnTree size={18} />,     label: 'Components' },
    { id: 'sim',     icon: <IcnSimulator size={18}/>, label: 'Simulator' },
    { id: 'perf',    icon: <IcnPerf size={18} />,     label: 'Profiler' },
    { id: 'git',     icon: <IcnGit size={18} />,      label: 'Source Control' },
  ];
  return (
    <div className="activity">
      <div style={{ width: 36, height: 36, display: 'grid', placeItems: 'center', marginBottom: 6 }}>
        <BrandMark size={22} />
      </div>
      {items.map(it => (
        <div
          key={it.id}
          className={'item ' + (view === it.id ? 'active' : '')}
          onClick={() => setView(it.id)}
          title={it.label}
        >
          {it.icon}
          {it.badge ? <span className="badge">{it.badge}</span> : null}
        </div>
      ))}
      <div className="spacer" />
      <div className="item" title="Command Palette" onClick={() => setView('cmd')}>
        <IcnCommand size={18} />
      </div>
      <div className="item" title="Settings">
        <IcnSettings size={18} />
      </div>
    </div>
  );
}

// ─────────────  File tree  ─────────────
function FileTreeNode({ node, depth, activePath, setActivePath }) {
  const [open, setOpen] = React.useState(node.open ?? false);
  const isFolder = node.type === 'folder';
  const path = node.name;
  const isActive = node.active;
  return (
    <>
      <div
        className={'tree-row ' + (isActive ? 'active ' : '') + (node.modified ? 'modified ' : '')}
        style={{ paddingLeft: 8 + depth * 12 }}
        onClick={() => isFolder ? setOpen(!open) : null}
      >
        <span className={'chev ' + (isFolder ? (open ? 'open' : '') : 'none')}>
          {isFolder ? '▶' : ''}
        </span>
        <span className="icn">
          {isFolder ? <IcnFolder open={open} /> : <FileIcon lang={node.lang} />}
        </span>
        <span className="name">{node.name}</span>
        {node.modified && !isFolder && <span className="dot-mod" />}
      </div>
      {isFolder && open && node.children && node.children.map((c, i) => (
        <FileTreeNode key={i} node={c} depth={depth + 1} />
      ))}
    </>
  );
}

function FileTree() {
  return (
    <div className="tree">
      <div className="tree-header">
        <span>{MOCK_PROJECT.name}</span>
        <span style={{ display: 'flex', gap: 4 }}>
          <span style={{ cursor: 'pointer', color: 'var(--tx-3)' }}>＋</span>
          <span style={{ cursor: 'pointer', color: 'var(--tx-3)' }}>···</span>
        </span>
      </div>
      {MOCK_FILE_TREE.map((n, i) => <FileTreeNode key={i} node={n} depth={0} />)}
      <div style={{ height: 14 }} />
      <div className="tree-header">Outline · cart.tsx</div>
      <div style={{ padding: '0 14px', fontSize: 11.5, color: 'var(--tx-3)', fontFamily: 'var(--font-mono)', lineHeight: '20px' }}>
        <div style={{ display: 'flex', gap: 6 }}>
          <span style={{ color: 'var(--sx-kw)' }}>fn</span>
          <span>CartScreen</span>
        </div>
        <div style={{ display: 'flex', gap: 6, paddingLeft: 14 }}>
          <span style={{ color: 'var(--info)' }}>hook</span>
          <span>useCart</span>
        </div>
        <div style={{ display: 'flex', gap: 6, paddingLeft: 14 }}>
          <span style={{ color: 'var(--info)' }}>state</span>
          <span>sheetOpen</span>
        </div>
        <div style={{ display: 'flex', gap: 6, paddingLeft: 14 }}>
          <span style={{ color: 'var(--info)' }}>effect</span>
          <span>items</span>
        </div>
      </div>
    </div>
  );
}

// ─────────────  Tabs  ─────────────
function TabBar({ tabs, activeTab, setActiveTab }) {
  return (
    <div className="tabs">
      {tabs.map((t, i) => (
        <div
          key={i}
          className={'tab ' + (t.path === activeTab ? 'active' : '')}
          onClick={() => setActiveTab(t.path)}
        >
          <FileIcon lang={t.name.split('.').pop()} size={12} />
          <span>{t.name}</span>
          {t.modified ? <span className="mod" /> : null}
          <span className="x" onClick={(e) => e.stopPropagation()}>
            <IcnX size={10} />
          </span>
        </div>
      ))}
      <div style={{ flex: 1, borderBottom: '1px solid var(--line-1)' }} />
      <div style={{
        display: 'flex', alignItems: 'center', padding: '0 10px',
        color: 'var(--tx-4)', fontSize: 11, gap: 10,
        borderBottom: '1px solid var(--line-1)',
      }}>
        <span style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
          <IcnLayout size={12} /> Split
        </span>
      </div>
    </div>
  );
}

// ─────────────  Code surface  ─────────────
function CodeView({ paused, breakpoints, activeLine, onToggleBreakpoint }) {
  const lines = MOCK_CODE_LINES;
  return (
    <div className="editor">
      <div className="gutter">
        {lines.map((_, i) => {
          const ln = i + 1;
          const hasBp = breakpoints.includes(ln);
          const isActive = paused && ln === activeLine;
          return (
            <div
              key={ln}
              className={'ln ' + (hasBp ? 'bp ' : '') + (isActive ? 'active' : '')}
              onClick={() => onToggleBreakpoint(ln)}
            >
              {hasBp ? <span className="bp-dot" /> : <span className="bp-stub" />}
              {!isActive && ln}
            </div>
          );
        })}
      </div>
      <div className="code">
        {lines.map((row, i) => {
          const ln = i + 1;
          const isActive = paused && ln === activeLine;
          const hasBp = breakpoints.includes(ln);
          // Inline value for line 12 (paused — items.length)
          const inlineVal = isActive ? '3' : null;
          return (
            <div key={ln} className={'row ' + (isActive ? 'active ' : '') + (hasBp ? 'bp' : '')}>
              {row.length === 0 ? '\u00A0' : row.map((tok, j) => (
                <span key={j} className={'t-' + tok.c}>{tok.t}</span>
              ))}
              {inlineVal && (
                <span className="inline-val">items.length = {inlineVal}</span>
              )}
            </div>
          );
        })}
      </div>
      <div className="minimap">
        {lines.map((row, i) => {
          const w = Math.min(95, Math.max(8, row.length * 6 + Math.random() * 20));
          return <div key={i} className="mm-row" style={{ width: w + '%' }} />;
        })}
        <div className="vp" style={{ top: 16, height: 200 }} />
      </div>
    </div>
  );
}

Object.assign(window, { ActivityBar, FileTree, TabBar, CodeView });
