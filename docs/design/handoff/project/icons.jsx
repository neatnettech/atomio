// atomio — shared icons (24x24 stroke, 1.5px) and small visual atoms.
// All inherit currentColor so token classes drive them.

const Icon = ({ d, size = 16, stroke = 1.5, fill = 'none', children, viewBox = '0 0 24 24', style = {} }) => (
  <svg width={size} height={size} viewBox={viewBox} fill={fill} stroke="currentColor" strokeWidth={stroke} strokeLinecap="round" strokeLinejoin="round" style={style}>
    {d ? <path d={d} /> : children}
  </svg>
);

// Brand mark — atomio: stylized "A" inside a hex orbit
const BrandMark = ({ size = 22, color }) => (
  <svg width={size} height={size} viewBox="0 0 32 32" fill="none" style={{ color: color || 'var(--accent)' }}>
    <circle cx="16" cy="16" r="4" fill="currentColor"/>
    <ellipse cx="16" cy="16" rx="12" ry="5" stroke="currentColor" strokeWidth="1.5" opacity="0.85"/>
    <ellipse cx="16" cy="16" rx="12" ry="5" stroke="currentColor" strokeWidth="1.5" opacity="0.5" transform="rotate(60 16 16)"/>
    <ellipse cx="16" cy="16" rx="12" ry="5" stroke="currentColor" strokeWidth="1.5" opacity="0.5" transform="rotate(-60 16 16)"/>
  </svg>
);

// Sidebar / activity icons
const IcnFiles    = (p) => <Icon {...p}><path d="M5 4h8l4 4v12a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1Z"/><path d="M13 4v4h4"/></Icon>;
const IcnSearch   = (p) => <Icon {...p}><circle cx="11" cy="11" r="6"/><path d="m20 20-4.3-4.3"/></Icon>;
const IcnDebug    = (p) => <Icon {...p}><circle cx="12" cy="13" r="5"/><path d="M12 8V5m0 3-2-2m2 2 2-2M7 18l-2 1m2-1-1 2m13-2 2 1m-2-1 1 2M5 13H3m18 0h-2m-9 5v3m2-3v3"/></Icon>;
const IcnSimulator= (p) => <Icon {...p}><rect x="6" y="3" width="12" height="18" rx="2.5"/><path d="M11 18h2"/></Icon>;
const IcnTree     = (p) => <Icon {...p}><circle cx="6" cy="6" r="2"/><circle cx="6" cy="18" r="2"/><circle cx="18" cy="12" r="2"/><path d="M8 6h2a4 4 0 0 1 4 4v0a2 2 0 0 0 2 2M8 18h2a4 4 0 0 0 4-4v0"/></Icon>;
const IcnPerf     = (p) => <Icon {...p}><path d="M3 17l5-5 4 4 8-8"/><path d="M14 8h6v6"/></Icon>;
const IcnConsole  = (p) => <Icon {...p}><rect x="3" y="5" width="18" height="14" rx="2"/><path d="m7 10 3 2-3 2m6 0h4"/></Icon>;
const IcnSettings = (p) => <Icon {...p}><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h0a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h0a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z"/></Icon>;
const IcnGit      = (p) => <Icon {...p}><circle cx="6" cy="6" r="2"/><circle cx="18" cy="18" r="2"/><circle cx="6" cy="18" r="2"/><path d="M6 8v8m2-10a6 6 0 0 1 6 6v4"/></Icon>;
const IcnCommand  = (p) => <Icon {...p}><path d="M9 6V4.5a2.5 2.5 0 1 0-2.5 2.5H9zm0 0v12m0-12h6m-6 12v-1.5a2.5 2.5 0 1 1 2.5 2.5H9zm6 0v1.5a2.5 2.5 0 1 0 2.5-2.5H15zm0 0V6m0 0h-1.5A2.5 2.5 0 1 1 16 3.5V6h-1z"/></Icon>;

// File-icon glyphs (small colored marks)
const IcnFolder   = ({ open = false, size = 14 }) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="none">
    <path d={open
      ? 'M2 5a1 1 0 0 1 1-1h3l1.5 1.5H13a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V5z'
      : 'M2 5a1 1 0 0 1 1-1h3l1.5 1.5H13a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V5z'}
      stroke="#7a8290" strokeWidth="1.2" fill={open ? 'rgba(122,130,144,0.15)' : 'transparent'}/>
  </svg>
);
const FileIcon = ({ lang, size = 14 }) => {
  const map = {
    tsx:  { bg: 'rgba(108,179,255,0.15)', fg: '#6cb3ff', label: 'TS' },
    ts:   { bg: 'rgba(108,179,255,0.15)', fg: '#6cb3ff', label: 'TS' },
    js:   { bg: 'rgba(255,180,84,0.15)',  fg: '#ffb454', label: 'JS' },
    json: { bg: 'rgba(155,216,127,0.13)', fg: '#9bd87f', label: '{}' },
    md:   { bg: 'rgba(255,138,138,0.13)', fg: '#ff8a8a', label: 'MD' },
  };
  const it = map[lang] || { bg: 'rgba(122,130,144,0.13)', fg: '#7a8290', label: '·' };
  return (
    <div style={{
      width: size, height: size, borderRadius: 3,
      background: it.bg, color: it.fg,
      fontSize: size < 14 ? 7 : 7.5, fontWeight: 700,
      display: 'grid', placeItems: 'center',
      fontFamily: 'var(--font-mono)', letterSpacing: '-0.04em',
    }}>{it.label}</div>
  );
};

// Debug control glyphs
const IcnPlay     = (p) => <Icon {...p} fill="currentColor" stroke="none"><path d="M7 4v16l13-8L7 4z"/></Icon>;
const IcnPause    = (p) => <Icon {...p} fill="currentColor" stroke="none"><rect x="6" y="4" width="4" height="16" rx="1"/><rect x="14" y="4" width="4" height="16" rx="1"/></Icon>;
const IcnStop     = (p) => <Icon {...p} fill="currentColor" stroke="none"><rect x="5" y="5" width="14" height="14" rx="1.5"/></Icon>;
const IcnStepOver = (p) => <Icon {...p}><path d="M5 12a7 7 0 0 1 13-3.5"/><path d="M18 4v5h-5"/><circle cx="12" cy="18" r="1.5" fill="currentColor"/></Icon>;
const IcnStepInto = (p) => <Icon {...p}><path d="M12 4v10"/><path d="m8 10 4 4 4-4"/><circle cx="12" cy="19" r="1.5" fill="currentColor"/></Icon>;
const IcnStepOut  = (p) => <Icon {...p}><path d="M12 14V4"/><path d="m8 8 4-4 4 4"/><circle cx="12" cy="19" r="1.5" fill="currentColor"/></Icon>;
const IcnRestart  = (p) => <Icon {...p}><path d="M3 12a9 9 0 1 0 3-6.7"/><path d="M3 4v5h5"/></Icon>;

// Other
const IcnX        = (p) => <Icon {...p}><path d="M6 6l12 12M18 6 6 18"/></Icon>;
const IcnPlus     = (p) => <Icon {...p}><path d="M12 5v14M5 12h14"/></Icon>;
const IcnChevDown = (p) => <Icon {...p}><path d="m6 9 6 6 6-6"/></Icon>;
const IcnChevRt   = (p) => <Icon {...p}><path d="m9 6 6 6-6 6"/></Icon>;
const IcnDot      = ({ size = 8, color = 'currentColor' }) => (
  <span style={{ width: size, height: size, borderRadius: '50%', background: color, display: 'inline-block' }} />
);
const IcnReload   = (p) => <Icon {...p}><path d="M21 12a9 9 0 1 1-3-6.7"/><path d="M21 4v5h-5"/></Icon>;
const IcnDevice   = (p) => <Icon {...p}><rect x="6" y="3" width="12" height="18" rx="2.5"/><circle cx="12" cy="18" r="0.5" fill="currentColor"/></Icon>;
const IcnLayout   = (p) => <Icon {...p}><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 10h18M9 10v10"/></Icon>;
const IcnInspect  = (p) => <Icon {...p}><path d="M3 3h7v7H3z"/><path d="M14 3h7v7h-7z"/><path d="M3 14h7v7H3z"/><path d="m14 14 7 7m0-7v7h-7"/></Icon>;
const IcnZap      = (p) => <Icon {...p} fill="currentColor"><path d="M13 2 4 14h7l-1 8 9-12h-7l1-8z"/></Icon>;
const IcnFilter   = (p) => <Icon {...p}><path d="M3 5h18l-7 8v6l-4-2v-4L3 5z"/></Icon>;
const IcnCheck    = (p) => <Icon {...p}><path d="m4 12 5 5L20 6"/></Icon>;

Object.assign(window, {
  Icon, BrandMark,
  IcnFiles, IcnSearch, IcnDebug, IcnSimulator, IcnTree, IcnPerf, IcnConsole, IcnSettings,
  IcnGit, IcnCommand, IcnFolder, FileIcon,
  IcnPlay, IcnPause, IcnStop, IcnStepOver, IcnStepInto, IcnStepOut, IcnRestart,
  IcnX, IcnPlus, IcnChevDown, IcnChevRt, IcnDot, IcnReload, IcnDevice, IcnLayout, IcnInspect,
  IcnZap, IcnFilter, IcnCheck,
});
