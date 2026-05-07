// atomio — branding canvas: app icon options + GitHub repo hero.
// Each artboard is a static SVG composition built with the brand
// vocabulary: green accent, near-black slate, atom/orbit metaphor.

const G = {
  bg: '#0c0e11',
  bg2: '#11141a',
  accent: '#3ecf8e',
  accentDim: '#2aa672',
  fg: '#e8ebf0',
  fg2: '#7a8290',
  fg3: '#525866',
  fontUI: '-apple-system, BlinkMacSystemFont, "SF Pro Display", "SF Pro", sans-serif',
  fontMono: 'ui-monospace, "SF Mono", "JetBrains Mono", Menlo, monospace',
};

// ─────────  Icon scaffold  ─────────
// Renders at 1024×1024 for an exported app icon; fits any container.
function IconFrame({ children, bg = G.bg, radius = 224 }) {
  return (
    <svg viewBox="0 0 1024 1024" width="100%" height="100%" style={{ display: 'block' }}>
      <defs>
        <clipPath id="cp-sq"><rect width="1024" height="1024" rx={radius} ry={radius}/></clipPath>
      </defs>
      <g clipPath="url(#cp-sq)">
        <rect width="1024" height="1024" fill={bg}/>
        {children}
      </g>
      {/* macOS-style inner light edge */}
      <rect x="2" y="2" width="1020" height="1020" rx={radius - 2} ry={radius - 2}
        fill="none" stroke="rgba(255,255,255,0.08)" strokeWidth="3"/>
    </svg>
  );
}

// ─────────  Icon 01 — Atom orbits (signature)  ─────────
function IconOrbits() {
  return (
    <IconFrame bg="#0c0e11">
      {/* very subtle radial glow */}
      <defs>
        <radialGradient id="g-glow" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="#3ecf8e" stopOpacity="0.18"/>
          <stop offset="60%" stopColor="#3ecf8e" stopOpacity="0"/>
        </radialGradient>
      </defs>
      <rect width="1024" height="1024" fill="url(#g-glow)"/>
      <g transform="translate(512 512)" stroke={G.accent} fill="none" strokeWidth="14" strokeLinecap="round">
        <ellipse rx="340" ry="130" opacity="0.95"/>
        <ellipse rx="340" ry="130" transform="rotate(60)" opacity="0.55"/>
        <ellipse rx="340" ry="130" transform="rotate(-60)" opacity="0.55"/>
      </g>
      {/* nucleus */}
      <circle cx="512" cy="512" r="92" fill={G.accent}/>
      <circle cx="512" cy="512" r="92" fill="none" stroke="#0c0e11" strokeWidth="6"/>
      {/* electron dots */}
      <circle cx="852" cy="512" r="22" fill={G.accent}/>
      <circle cx="342" cy="276" r="14" fill="#9bd87f" opacity="0.85"/>
      <circle cx="682" cy="748" r="14" fill="#6cb3ff" opacity="0.85"/>
    </IconFrame>
  );
}

// ─────────  Icon 02 — Bracket terminal (codey)  ─────────
function IconBracket() {
  return (
    <IconFrame bg="#11141a">
      {/* diagonal accent strip */}
      <rect x="0" y="0" width="1024" height="1024" fill="#0c0e11"/>
      <g transform="translate(512 512)">
        <text textAnchor="middle" dominantBaseline="central"
          y="14"
          fontFamily={G.fontMono} fontWeight="700" fontSize="540"
          fill={G.fg} letterSpacing="-30">
          {'{ }'}
        </text>
        {/* breakpoint dot */}
        <circle cx="-300" cy="-300" r="44" fill="#ff6b6b"/>
        {/* run dot */}
        <circle cx="300" cy="300" r="44" fill={G.accent}/>
      </g>
    </IconFrame>
  );
}

// ─────────  Icon 03 — Lowercase 'a' marque  ─────────
function IconLetter() {
  return (
    <IconFrame bg={G.accent}>
      <defs>
        <linearGradient id="lg-a" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="#5fe0a3"/>
          <stop offset="100%" stopColor="#2aa672"/>
        </linearGradient>
      </defs>
      <rect width="1024" height="1024" fill="url(#lg-a)"/>
      {/* set 'a' in heavy weight, with a chunky cut-out terminal */}
      <text x="512" y="612" textAnchor="middle"
        fontFamily={G.fontUI} fontWeight="800" fontSize="780"
        fill="#062416" letterSpacing="-40">a</text>
      {/* breakpoint pin like a punctuation mark */}
      <circle cx="780" cy="640" r="56" fill="#062416"/>
      <circle cx="780" cy="640" r="22" fill={G.accent}/>
    </IconFrame>
  );
}

// ─────────  Icon 04 — Phone-in-window (literal)  ─────────
function IconDevice() {
  return (
    <IconFrame bg="#0c0e11">
      {/* window chrome */}
      <rect x="120" y="180" width="784" height="664" rx="64" fill="#161a21" stroke="rgba(255,255,255,0.08)" strokeWidth="3"/>
      {/* traffic lights */}
      <circle cx="186" cy="246" r="18" fill="#ff5f57"/>
      <circle cx="240" cy="246" r="18" fill="#febc2e"/>
      <circle cx="294" cy="246" r="18" fill="#28c941"/>
      {/* phone */}
      <rect x="380" y="320" width="264" height="468" rx="46" fill="#0a0c0f" stroke="#232934" strokeWidth="6"/>
      <rect x="404" y="344" width="216" height="420" rx="30" fill="#fafafa"/>
      {/* phone notch */}
      <rect x="478" y="350" width="68" height="14" rx="7" fill="#0a0c0f"/>
      {/* screen content lines */}
      <rect x="424" y="404" width="120" height="22" rx="6" fill="#0a0c0f"/>
      <rect x="424" y="448" width="176" height="44" rx="10" fill="#eee"/>
      <rect x="424" y="500" width="176" height="44" rx="10" fill="#eee"/>
      <rect x="424" y="552" width="176" height="44" rx="10" fill="#eee"/>
      <rect x="424" y="700" width="176" height="40" rx="12" fill={G.accent}/>
      {/* live dot */}
      <circle cx="844" cy="246" r="14" fill={G.accent}>
        <animate attributeName="opacity" values="1;0.3;1" dur="1.8s" repeatCount="indefinite"/>
      </circle>
    </IconFrame>
  );
}

// ─────────  Icon 05 — Pulse / waveform  ─────────
function IconPulse() {
  return (
    <IconFrame bg="#0c0e11">
      <defs>
        <linearGradient id="lg-p" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0%" stopColor="#3ecf8e" stopOpacity="0.2"/>
          <stop offset="100%" stopColor="#3ecf8e" stopOpacity="0"/>
        </linearGradient>
      </defs>
      <rect x="0" y="492" width="1024" height="40" fill="url(#lg-p)"/>
      <path d="M 80 512 L 280 512 L 340 380 L 420 660 L 500 280 L 580 740 L 680 512 L 944 512"
        fill="none" stroke={G.accent} strokeWidth="22"
        strokeLinecap="round" strokeLinejoin="round"/>
      {/* breakpoint marker on peak */}
      <circle cx="500" cy="280" r="34" fill="#0c0e11" stroke={G.accent} strokeWidth="10"/>
    </IconFrame>
  );
}

// ─────────  Icon 06 — Cursor/caret monogram  ─────────
function IconCaret() {
  return (
    <IconFrame bg="#0c0e11">
      {/* big slash bar */}
      <g transform="translate(512 512) rotate(-12)">
        <rect x="-44" y="-300" width="88" height="600" rx="44" fill={G.accent}/>
      </g>
      {/* "ai" hint as small caret marks at top and bottom */}
      <g fill={G.fg}>
        <rect x="160" y="160" width="120" height="20" rx="10"/>
        <rect x="200" y="200" width="40" height="20" rx="10"/>
        <rect x="744" y="844" width="120" height="20" rx="10"/>
        <rect x="784" y="804" width="40" height="20" rx="10"/>
      </g>
      <text x="512" y="612" textAnchor="middle"
        fontFamily={G.fontMono} fontWeight="700" fontSize="280"
        fill="#062416" letterSpacing="-10" transform="rotate(-12 512 580)">ai</text>
    </IconFrame>
  );
}

// ─────────  GitHub Hero  ─────────
function GitHubHero() {
  return (
    <svg viewBox="0 0 1280 640" width="100%" height="100%" style={{ display: 'block' }}>
      <defs>
        <radialGradient id="hero-bg" cx="30%" cy="20%" r="80%">
          <stop offset="0%" stopColor="#15211c"/>
          <stop offset="60%" stopColor="#0c0e11"/>
          <stop offset="100%" stopColor="#07090b"/>
        </radialGradient>
        <radialGradient id="hero-glow" cx="22%" cy="80%" r="40%">
          <stop offset="0%" stopColor="#3ecf8e" stopOpacity="0.22"/>
          <stop offset="100%" stopColor="#3ecf8e" stopOpacity="0"/>
        </radialGradient>
        <linearGradient id="hero-edge" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="rgba(255,255,255,0.05)"/>
          <stop offset="100%" stopColor="rgba(255,255,255,0)"/>
        </linearGradient>
        <pattern id="hero-grid" width="32" height="32" patternUnits="userSpaceOnUse">
          <path d="M32 0H0V32" fill="none" stroke="rgba(255,255,255,0.025)" strokeWidth="1"/>
        </pattern>
      </defs>
      <rect width="1280" height="640" fill="url(#hero-bg)"/>
      <rect width="1280" height="640" fill="url(#hero-grid)"/>
      <rect width="1280" height="640" fill="url(#hero-glow)"/>

      {/* Left column — wordmark + tagline + meta */}
      <g transform="translate(80 120)">
        {/* Brand mark */}
        <g transform="translate(0 0)">
          <circle cx="32" cy="32" r="11" fill={G.accent}/>
          <ellipse cx="32" cy="32" rx="28" ry="11" fill="none" stroke={G.accent} strokeWidth="3"/>
          <ellipse cx="32" cy="32" rx="28" ry="11" fill="none" stroke={G.accent} strokeOpacity="0.6" strokeWidth="3" transform="rotate(60 32 32)"/>
          <ellipse cx="32" cy="32" rx="28" ry="11" fill="none" stroke={G.accent} strokeOpacity="0.6" strokeWidth="3" transform="rotate(-60 32 32)"/>
        </g>
        <text x="80" y="44" fontFamily={G.fontUI} fontWeight="700" fontSize="34" fill={G.fg} letterSpacing="-1.2">atomio</text>
        <text x="80" y="68" fontFamily={G.fontMono} fontSize="13" fill={G.fg3} letterSpacing="2">v 0.4.2 · MIT · macOS 13+</text>

        {/* Headline */}
        <text fontFamily={G.fontUI} fontWeight="600" fontSize="62" letterSpacing="-2.5" fill={G.fg}>
          <tspan x="0" y="190">React Native &amp; Expo</tspan>
          <tspan x="0" y="270" fill={G.fg2}>debugging that gets</tspan>
          <tspan x="0" y="350" fill={G.accent}>out of your way.</tspan>
        </text>

        {/* Subhead */}
        <text fontFamily={G.fontUI} fontSize="20" fill={G.fg2}>
          <tspan x="0" y="334">Editor, breakpoints, simulator, profiler —</tspan>
          <tspan x="0" y="362">one window. Built native for macOS.</tspan>
        </text>

        {/* CTAs */}
        <g transform="translate(0 410)">
          {/* primary */}
          <rect x="0" y="0" width="172" height="46" rx="8" fill={G.accent}/>
          <text x="86" y="30" textAnchor="middle" fontFamily={G.fontUI} fontWeight="600" fontSize="15" fill="#062416">↓  Download for Mac</text>
          {/* secondary */}
          <rect x="184" y="0" width="158" height="46" rx="8" fill="none" stroke="rgba(255,255,255,0.18)"/>
          <text x="263" y="30" textAnchor="middle" fontFamily={G.fontUI} fontWeight="500" fontSize="14" fill={G.fg}>★  Star on GitHub</text>
          <text x="0" y="80" fontFamily={G.fontMono} fontSize="12" fill={G.fg3}>brew install --cask atomio</text>
        </g>
      </g>

      {/* Right column — floating window mock (3/4 perspective) */}
      <g transform="translate(720 70)">
        {/* shadow */}
        <ellipse cx="240" cy="500" rx="240" ry="22" fill="#000" opacity="0.5"/>
        <g transform="skewY(-3)">
          {/* window */}
          <rect x="0" y="0" width="500" height="380" rx="14" fill={G.bg2} stroke="rgba(255,255,255,0.08)"/>
          <rect x="0" y="0" width="500" height="380" rx="14" fill="url(#hero-edge)"/>
          {/* titlebar */}
          <rect x="0" y="0" width="500" height="30" rx="14" fill="#1a1f27"/>
          <rect x="0" y="22" width="500" height="8" fill="#1a1f27"/>
          <circle cx="18" cy="15" r="5" fill="#ff5f57"/>
          <circle cx="34" cy="15" r="5" fill="#febc2e"/>
          <circle cx="50" cy="15" r="5" fill="#28c941"/>
          <text x="250" y="19" textAnchor="middle" fontFamily={G.fontUI} fontSize="10" fill={G.fg3}>cart.tsx — atomio</text>
          {/* activity bar */}
          <rect x="0" y="30" width="32" height="350" fill="#0f1217"/>
          {[0, 1, 2, 3, 4].map(i => (
            <rect key={i} x="8" y={48 + i * 36} width="16" height="16" rx="3" fill={i === 2 ? G.accent : G.fg3} opacity={i === 2 ? 1 : 0.4}/>
          ))}
          {/* file tree */}
          <rect x="32" y="30" width="100" height="350" fill="#161a21"/>
          {['app', '  cart.tsx', '  index.tsx', 'components', '  CartItem', 'hooks', 'package.json'].map((n, i) => (
            <text key={i} x="40" y={56 + i * 18} fontFamily={G.fontMono} fontSize="9" fill={i === 1 ? G.accent : G.fg2}>{n}</text>
          ))}
          {/* tabs */}
          <rect x="132" y="30" width="368" height="22" fill="#11141a"/>
          <rect x="132" y="30" width="86" height="22" fill={G.bg2}/>
          <rect x="132" y="30" width="86" height="2" fill={G.accent}/>
          <text x="142" y="45" fontFamily={G.fontMono} fontSize="9" fill={G.fg}>cart.tsx ●</text>
          <text x="226" y="45" fontFamily={G.fontMono} fontSize="9" fill={G.fg3}>useCart.ts</text>
          {/* gutter + active line */}
          <rect x="132" y="52" width="22" height="200" fill="#11141a"/>
          <circle cx="143" cy="105" r="3.5" fill="#ff6b6b"/>
          {/* active line highlight */}
          <rect x="154" y="100" width="346" height="14" fill="rgba(62,207,142,0.08)"/>
          <rect x="154" y="100" width="2" height="14" fill={G.accent}/>
          <text x="160" y="111" fontFamily={G.fontMono} fontSize="9" fill={G.accent}>→</text>
          {/* code lines */}
          {[
            { y: 65,  parts: [['import ', '#d18cff'], ['React', '#c8cfd9'], [' from ', '#97a0ad'], ["'react'", '#9bd87f']] },
            { y: 79,  parts: [['import ', '#d18cff'], ['{ View, Text } ', '#c8cfd9'], ['from ', '#97a0ad'], ["'react-native'", '#9bd87f']] },
            { y: 93,  parts: [['', '']] },
            { y: 111, parts: [['  ', ''], ['useEffect', '#6cb3ff'], ['(() => {', '#97a0ad']], active: true },
            { y: 125, parts: [['    console.log(', '#97a0ad'], ["'[cart]'", '#9bd87f'], [', items.length)', '#97a0ad']] },
            { y: 139, parts: [['  }, [items])', '#97a0ad']] },
            { y: 157, parts: [['  return (', '#d18cff']] },
            { y: 171, parts: [['    <', '#7a8290'], ['View ', '#ff8a8a'], ['style', '#ffd58a'], ['={s.box}>', '#97a0ad']] },
            { y: 185, parts: [['      <', '#7a8290'], ['FlatList ', '#ff8a8a'], ['data', '#ffd58a'], ['={items} />', '#97a0ad']] },
            { y: 199, parts: [['    </', '#7a8290'], ['View', '#ff8a8a'], ['>', '#7a8290']] },
            { y: 213, parts: [['  )', '#97a0ad']] },
            { y: 227, parts: [['}', '#97a0ad']] },
          ].map((row, i) => (
            <text key={i} x="172" y={row.y} fontFamily={G.fontMono} fontSize="9">
              {row.parts.map((p, j) => <tspan key={j} fill={p[1] || G.fg2}>{p[0]}</tspan>)}
              {row.active && <tspan dx="8" fill={G.accent} fontSize="8">items.length = 3</tspan>}
            </text>
          ))}
          {/* line numbers */}
          {[1,2,3,4,5,6,7,8,9,10,11,12].map(n => (
            <text key={n} x="148" y={65 + (n-1) * 14} textAnchor="end" fontFamily={G.fontMono} fontSize="8" fill="#363b46">{n}</text>
          ))}

          {/* bottom: console */}
          <rect x="132" y="252" width="368" height="128" fill="#0e1116"/>
          <rect x="132" y="252" width="368" height="18" fill="#161a21"/>
          <text x="140" y="264" fontFamily={G.fontMono} fontSize="8" fill={G.fg2}>Console · 142 logs</text>
          <circle cx="486" cy="261" r="2.5" fill={G.accent}/>
          {[
            { c: '#6cb3ff', t: 'INFO',  m: 'Bundling complete · 1842ms · 218 modules' },
            { c: '#7a8290', t: 'LOG',   m: '[cart] items changed 3' },
            { c: '#7a8290', t: 'LOG',   m: 'Added to cart: Flat White (medium)' },
            { c: '#ffb454', t: 'WARN',  m: 'VirtualizedLists nested in ScrollView' },
            { c: '#ff6b6b', t: 'ERROR', m: 'TypeError: Cannot read property "id"' },
            { c: '#7a8290', t: 'LOG',   m: '⏸︎ Paused on breakpoint cart.tsx:12' },
          ].map((l, i) => (
            <g key={i} transform={`translate(140 ${280 + i * 14})`}>
              <text x="0" y="0" fontFamily={G.fontMono} fontSize="7" fill="#363b46">14:02:{31 - i * 2}</text>
              <text x="44" y="0" fontFamily={G.fontMono} fontSize="7" fill={l.c} fontWeight="700">{l.t}</text>
              <text x="76" y="0" fontFamily={G.fontMono} fontSize="7" fill={i === 4 ? '#ff6b6b' : G.fg}>{l.m}</text>
            </g>
          ))}
        </g>

        {/* floating phone, slightly overlapping */}
        <g transform="translate(380 240)">
          <rect x="-4" y="-4" width="148" height="288" rx="28" fill="#0a0c0f" stroke="#232934" strokeWidth="3"/>
          <rect x="6" y="6" width="128" height="268" rx="20" fill="#fafafa"/>
          <rect x="50" y="10" width="40" height="8" rx="4" fill="#0a0c0f"/>
          <text x="70" y="44" textAnchor="middle" fontFamily={G.fontUI} fontWeight="700" fontSize="11" fill="#0a0c0f">Your Cart</text>
          {[0, 1, 2].map(i => (
            <g key={i} transform={`translate(14 ${60 + i * 38})`}>
              <rect x="0" y="0" width="22" height="22" rx="6" fill={['#7d5a3f','#a07f5c','#5a3e2b'][i]}/>
              <rect x="28" y="2" width="56" height="6" rx="3" fill="#0a0c0f"/>
              <rect x="28" y="12" width="40" height="4" rx="2" fill="#bbb"/>
              <text x="112" y="14" textAnchor="end" fontFamily={G.fontUI} fontSize="8" fontWeight="700" fill="#0a0c0f">$4.50</text>
            </g>
          ))}
          <rect x="14" y="220" width="112" height="28" rx="9" fill="#0a0c0f"/>
          <text x="22" y="238" fontFamily={G.fontUI} fontSize="9" fontWeight="600" fill="#fff">Checkout</text>
          <text x="118" y="238" textAnchor="end" fontFamily={G.fontUI} fontSize="9" fontWeight="600" fill="#fff">$14.50</text>
          {/* live indicator */}
          <circle cx="135" cy="-10" r="6" fill={G.accent}/>
          <circle cx="135" cy="-10" r="10" fill="none" stroke={G.accent} strokeOpacity="0.4" strokeWidth="2"/>
        </g>
      </g>

      {/* Footer chips */}
      <g transform="translate(80 580)">
        {['Editor', 'Breakpoints', 'iOS Simulator', 'Profiler', 'Component Tree', '⌘K Palette'].map((t, i) => (
          <g key={t} transform={`translate(${i * 124} 0)`}>
            <rect x="0" y="0" width="112" height="26" rx="13" fill="rgba(255,255,255,0.04)" stroke="rgba(255,255,255,0.08)"/>
            <circle cx="14" cy="13" r="3" fill={G.accent}/>
            <text x="26" y="17" fontFamily={G.fontUI} fontSize="11" fill={G.fg2}>{t}</text>
          </g>
        ))}
      </g>
    </svg>
  );
}

// ─────────  Wide GitHub social card (1280×640) — alt minimal version  ─────────
function GitHubSocialCard() {
  return (
    <svg viewBox="0 0 1280 640" width="100%" height="100%" style={{ display: 'block' }}>
      <defs>
        <radialGradient id="sc-bg" cx="50%" cy="50%" r="60%">
          <stop offset="0%" stopColor="#13191e"/>
          <stop offset="100%" stopColor="#07090b"/>
        </radialGradient>
        <radialGradient id="sc-pulse" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="#3ecf8e" stopOpacity="0.3"/>
          <stop offset="60%" stopColor="#3ecf8e" stopOpacity="0"/>
        </radialGradient>
      </defs>
      <rect width="1280" height="640" fill="url(#sc-bg)"/>
      {/* concentric orbits */}
      <g transform="translate(640 320)" stroke="#3ecf8e" fill="none" strokeWidth="2" opacity="0.18">
        <circle r="120"/>
        <circle r="200"/>
        <circle r="280"/>
        <circle r="360"/>
      </g>
      <ellipse cx="640" cy="320" rx="520" ry="200" fill="url(#sc-pulse)"/>

      {/* Big mark */}
      <g transform="translate(640 270)">
        <ellipse rx="180" ry="68" fill="none" stroke={G.accent} strokeWidth="6"/>
        <ellipse rx="180" ry="68" fill="none" stroke={G.accent} strokeOpacity="0.6" strokeWidth="6" transform="rotate(60)"/>
        <ellipse rx="180" ry="68" fill="none" stroke={G.accent} strokeOpacity="0.6" strokeWidth="6" transform="rotate(-60)"/>
        <circle r="46" fill={G.accent}/>
      </g>
      {/* wordmark */}
      <text x="640" y="450" textAnchor="middle" fontFamily={G.fontUI} fontWeight="700" fontSize="92" letterSpacing="-3" fill={G.fg}>atomio</text>
      <text x="640" y="494" textAnchor="middle" fontFamily={G.fontUI} fontSize="20" fill={G.fg2}>The React Native &amp; Expo debugger for Mac</text>

      {/* meta strip */}
      <g transform="translate(640 558)" textAnchor="middle">
        <text fontFamily={G.fontMono} fontSize="13" fill={G.fg3} letterSpacing="3">
          MIT · v 0.4.2 · macOS 13+ · github.com/atomio/atomio
        </text>
      </g>
    </svg>
  );
}

// ─────────  Page  ─────────
function Page() {
  return (
    <DesignCanvas>
      <DCSection id="icons" title="App icon" subtitle="Six directions — pick a favourite or mix elements">
        <DCArtboard id="orbit"   label="01 · Orbits (signature)"      width={220} height={220}><IconOrbits /></DCArtboard>
        <DCArtboard id="bracket" label="02 · Brackets"                width={220} height={220}><IconBracket /></DCArtboard>
        <DCArtboard id="letter"  label="03 · Lowercase a"             width={220} height={220}><IconLetter /></DCArtboard>
        <DCArtboard id="device"  label="04 · Window + phone"          width={220} height={220}><IconDevice /></DCArtboard>
        <DCArtboard id="pulse"   label="05 · Pulse"                   width={220} height={220}><IconPulse /></DCArtboard>
        <DCArtboard id="caret"   label="06 · Caret monogram"          width={220} height={220}><IconCaret /></DCArtboard>
      </DCSection>

      <DCSection id="hero" title="GitHub repo hero" subtitle="Top-of-README banner — 1280×640">
        <DCArtboard id="hero-product" label="Product hero · screenshot + copy" width={1280} height={640}><GitHubHero /></DCArtboard>
      </DCSection>

      <DCSection id="social" title="Social card" subtitle="OpenGraph / repo social preview — 1280×640">
        <DCArtboard id="hero-mark" label="Wordmark card · centered" width={1280} height={640}><GitHubSocialCard /></DCArtboard>
      </DCSection>
    </DesignCanvas>
  );
}

const root = ReactDOM.createRoot(document.getElementById('root'));
root.render(<Page />);
