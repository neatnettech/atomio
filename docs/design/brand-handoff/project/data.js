// Mock data for atomio — Expo/RN debugger prototype

window.MOCK_PROJECT = {
  name: 'expo-coffee-app',
  path: '~/Developer/expo-coffee-app',
  branch: 'feat/cart-checkout',
  device: 'iPhone 15 Pro · iOS 17.4',
  bundler: 'Metro · running on :8081',
};

window.MOCK_RECENT_PROJECTS = [
  { name: 'expo-coffee-app',    path: '~/Developer/expo-coffee-app',   accent: '#3ecf8e', last: '2m ago',  branch: 'feat/cart-checkout' },
  { name: 'rn-finance-tracker', path: '~/Developer/rn-finance',        accent: '#ff8a3d', last: '3h ago',  branch: 'main' },
  { name: 'pocket-journal',     path: '~/code/pocket-journal',         accent: '#7c5cff', last: 'yesterday', branch: 'redesign' },
  { name: 'beacon-runner',      path: '~/code/beacon-runner',          accent: '#4aa3ff', last: '4d ago',  branch: 'main' },
  { name: 'tide-weather',       path: '~/Sites/tide-weather',          accent: '#ff5c8a', last: 'last week', branch: 'main' },
];

window.MOCK_FILE_TREE = [
  { type: 'folder', name: 'app', open: true, children: [
    { type: 'folder', name: '(tabs)', open: true, children: [
      { type: 'file', name: '_layout.tsx', lang: 'tsx' },
      { type: 'file', name: 'index.tsx', lang: 'tsx' },
      { type: 'file', name: 'cart.tsx', lang: 'tsx', active: true, modified: true },
      { type: 'file', name: 'profile.tsx', lang: 'tsx' },
    ]},
    { type: 'file', name: '_layout.tsx', lang: 'tsx' },
    { type: 'file', name: '+not-found.tsx', lang: 'tsx' },
  ]},
  { type: 'folder', name: 'components', open: true, children: [
    { type: 'file', name: 'CartItem.tsx', lang: 'tsx' },
    { type: 'file', name: 'CheckoutSheet.tsx', lang: 'tsx', modified: true },
    { type: 'file', name: 'PriceLabel.tsx', lang: 'tsx' },
    { type: 'file', name: 'ThemedView.tsx', lang: 'tsx' },
  ]},
  { type: 'folder', name: 'hooks', open: false, children: [
    { type: 'file', name: 'useCart.ts', lang: 'ts' },
    { type: 'file', name: 'useColorScheme.ts', lang: 'ts' },
  ]},
  { type: 'folder', name: 'assets', open: false, children: [] },
  { type: 'file', name: 'app.json', lang: 'json' },
  { type: 'file', name: 'package.json', lang: 'json' },
  { type: 'file', name: 'tsconfig.json', lang: 'json' },
  { type: 'file', name: 'README.md', lang: 'md' },
];

window.MOCK_OPEN_TABS = [
  { name: 'cart.tsx',          path: 'app/(tabs)/cart.tsx',         modified: true,  active: true },
  { name: 'CheckoutSheet.tsx', path: 'components/CheckoutSheet.tsx', modified: true,  active: false },
  { name: 'useCart.ts',        path: 'hooks/useCart.ts',             modified: false, active: false },
  { name: 'app.json',          path: 'app.json',                     modified: false, active: false },
];

// Syntax-highlighted lines: each line is array of {t: text, c: color-token}
// tokens: kw (keyword), str (string), num, fn, comp (JSX component), prop, type, com (comment), pl (plain), op (operator), brk (bracket), brand (brand)
window.MOCK_CODE_LINES = [
  [{t:'import ',c:'kw'},{t:'React',c:'pl'},{t:', { ',c:'op'},{t:'useState',c:'fn'},{t:', ',c:'op'},{t:'useEffect',c:'fn'},{t:' } ',c:'op'},{t:'from ',c:'kw'},{t:"'react'",c:'str'}],
  [{t:'import ',c:'kw'},{t:'{ View, Text, Pressable, FlatList } ',c:'pl'},{t:'from ',c:'kw'},{t:"'react-native'",c:'str'}],
  [{t:'import ',c:'kw'},{t:'{ useCart } ',c:'pl'},{t:'from ',c:'kw'},{t:"'@/hooks/useCart'",c:'str'}],
  [{t:'import ',c:'kw'},{t:'CartItem ',c:'pl'},{t:'from ',c:'kw'},{t:"'@/components/CartItem'",c:'str'}],
  [{t:'import ',c:'kw'},{t:'CheckoutSheet ',c:'pl'},{t:'from ',c:'kw'},{t:"'@/components/CheckoutSheet'",c:'str'}],
  [],
  [{t:'export default function ',c:'kw'},{t:'CartScreen',c:'fn'},{t:'() {',c:'op'}],
  [{t:'  const ',c:'kw'},{t:'{ items, total, remove, checkout } = ',c:'pl'},{t:'useCart',c:'fn'},{t:'()',c:'op'}],
  [{t:'  const [',c:'kw'},{t:'sheetOpen',c:'pl'},{t:', ',c:'op'},{t:'setSheetOpen',c:'pl'},{t:'] = ',c:'op'},{t:'useState',c:'fn'},{t:'(',c:'op'},{t:'false',c:'kw'},{t:')',c:'op'}],
  [],
  [{t:'  ',c:'pl'},{t:'useEffect',c:'fn'},{t:'(() => {',c:'op'}],
  [{t:'    ',c:'pl'},{t:'console',c:'pl'},{t:'.',c:'op'},{t:'log',c:'fn'},{t:'(',c:'op'},{t:"'[cart] items changed'",c:'str'},{t:', items.length)',c:'op'}],
  [{t:'  }, [items])',c:'op'}],
  [],
  [{t:'  ',c:'kw'},{t:'if ',c:'kw'},{t:'(items.length === ',c:'op'},{t:'0',c:'num'},{t:') ',c:'op'},{t:'return ',c:'kw'},{t:'<',c:'brk'},{t:'EmptyCart ',c:'comp'},{t:'/>',c:'brk'}],
  [],
  [{t:'  return (',c:'kw'}],
  [{t:'    <',c:'brk'},{t:'View ',c:'comp'},{t:'style',c:'prop'},{t:'={styles.container}>',c:'op'}],
  [{t:'      <',c:'brk'},{t:'Text ',c:'comp'},{t:'style',c:'prop'},{t:'={styles.title}>',c:'op'},{t:'Your Cart',c:'pl'},{t:'</',c:'brk'},{t:'Text',c:'comp'},{t:'>',c:'brk'}],
  [{t:'      <',c:'brk'},{t:'FlatList ',c:'comp'}],
  [{t:'        ',c:'prop'},{t:'data',c:'prop'},{t:'={items}',c:'op'}],
  [{t:'        ',c:'prop'},{t:'keyExtractor',c:'prop'},{t:'={(i) => i.id}',c:'op'}],
  [{t:'        ',c:'prop'},{t:'renderItem',c:'prop'},{t:'={({ item }) => (',c:'op'}],
  [{t:'          <',c:'brk'},{t:'CartItem ',c:'comp'},{t:'item',c:'prop'},{t:'={item} ',c:'op'},{t:'onRemove',c:'prop'},{t:'={remove} ',c:'op'},{t:'/>',c:'brk'}],
  [{t:'        )}',c:'op'}],
  [{t:'      />',c:'brk'}],
  [{t:'      <',c:'brk'},{t:'Pressable ',c:'comp'},{t:'onPress',c:'prop'},{t:'={() => ',c:'op'},{t:'setSheetOpen',c:'fn'},{t:'(',c:'op'},{t:'true',c:'kw'},{t:')}>',c:'op'}],
  [{t:'        <',c:'brk'},{t:'Text',c:'comp'},{t:'>',c:'brk'},{t:'Checkout · $',c:'pl'},{t:'{total.toFixed(',c:'op'},{t:'2',c:'num'},{t:')}',c:'op'},{t:'</',c:'brk'},{t:'Text',c:'comp'},{t:'>',c:'brk'}],
  [{t:'      </',c:'brk'},{t:'Pressable',c:'comp'},{t:'>',c:'brk'}],
  [{t:'      <',c:'brk'},{t:'CheckoutSheet ',c:'comp'},{t:'open',c:'prop'},{t:'={sheetOpen} ',c:'op'},{t:'onClose',c:'prop'},{t:'={() => ',c:'op'},{t:'setSheetOpen',c:'fn'},{t:'(',c:'op'},{t:'false',c:'kw'},{t:')} ',c:'op'},{t:'/>',c:'brk'}],
  [{t:'    </',c:'brk'},{t:'View',c:'comp'},{t:'>',c:'brk'}],
  [{t:'  )',c:'op'}],
  [{t:'}',c:'op'}],
];

// Lines with breakpoints set + the active execution line
window.MOCK_BREAKPOINTS = [12, 15, 27];
window.MOCK_ACTIVE_LINE = 12; // currently paused here

window.MOCK_CALL_STACK = [
  { fn: 'CartScreen.useEffect',    file: 'app/(tabs)/cart.tsx',         line: 12,  current: true },
  { fn: 'commitHookEffectListMount', file: 'react-dom.development.js',  line: 23045 },
  { fn: 'commitPassiveMountEffects', file: 'react-dom.development.js',  line: 24890 },
  { fn: 'flushPassiveEffects',     file: 'react-dom.development.js',    line: 25001 },
  { fn: 'performSyncWorkOnRoot',   file: 'react-dom.development.js',    line: 24122 },
  { fn: '<anonymous>',             file: 'scheduler.development.js',    line: 142  },
];

window.MOCK_VARIABLES = [
  { scope: 'Local', items: [
    { name: 'items',       type: 'Array(3)',     value: '[{id:"a"}, {id:"b"}, {id:"c"}]', expandable: true },
    { name: 'total',       type: 'number',       value: '14.50' },
    { name: 'sheetOpen',   type: 'boolean',      value: 'false' },
    { name: 'remove',      type: 'function',     value: 'ƒ remove(id)' },
    { name: 'checkout',    type: 'function',     value: 'ƒ checkout()' },
  ]},
  { scope: 'Closure', items: [
    { name: 'CartContext', type: 'Object',       value: '{provider, consumer}', expandable: true },
    { name: 'navigation',  type: 'Object',       value: '{navigate, push, …}',  expandable: true },
  ]},
  { scope: 'Global', items: [
    { name: '__DEV__',     type: 'boolean',      value: 'true' },
    { name: 'global',      type: 'Object',       value: 'Window',                expandable: true },
  ]},
];

window.MOCK_BREAKPOINT_LIST = [
  { file: 'cart.tsx',         line: 12, expr: 'console.log',           enabled: true,  hits: 14 },
  { file: 'cart.tsx',         line: 15, expr: 'if (items.length…)',    enabled: true,  hits: 3  },
  { file: 'cart.tsx',         line: 27, expr: 'setSheetOpen(true)',    enabled: false, hits: 0  },
  { file: 'CheckoutSheet.tsx',line: 48, expr: 'submitOrder()',         enabled: true,  hits: 1  },
];

window.MOCK_LOGS = [
  { t: '14:02:11.204', lvl: 'info',  src: 'metro',   msg: 'Bundling complete · 1842ms · 218 modules' },
  { t: '14:02:11.842', lvl: 'info',  src: 'expo',    msg: 'Running app on iPhone 15 Pro' },
  { t: '14:02:12.110', lvl: 'log',   src: 'app',     msg: '[cart] items changed 0' },
  { t: '14:02:14.391', lvl: 'log',   src: 'app',     msg: '[cart] items changed 1' },
  { t: '14:02:14.395', lvl: 'log',   src: 'app',     msg: 'Added to cart: Flat White (medium)' },
  { t: '14:02:18.022', lvl: 'log',   src: 'app',     msg: '[cart] items changed 2' },
  { t: '14:02:18.026', lvl: 'log',   src: 'app',     msg: 'Added to cart: Iced Latte (large)' },
  { t: '14:02:21.557', lvl: 'warn',  src: 'rn',      msg: 'VirtualizedLists should never be nested inside ScrollViews' },
  { t: '14:02:24.193', lvl: 'log',   src: 'app',     msg: '[cart] items changed 3' },
  { t: '14:02:24.198', lvl: 'log',   src: 'app',     msg: 'Added to cart: Cortado (small)' },
  { t: '14:02:27.002', lvl: 'debug', src: 'metro',   msg: 'HMR · components/CheckoutSheet.tsx' },
  { t: '14:02:27.401', lvl: 'log',   src: 'app',     msg: 'Reconciled 4 components in 8ms' },
  { t: '14:02:31.118', lvl: 'error', src: 'app',     msg: 'TypeError: Cannot read property "id" of undefined' },
  { t: '14:02:31.119', lvl: 'error', src: 'app',     msg: '    at CartItem (components/CartItem.tsx:23:12)' },
  { t: '14:02:31.119', lvl: 'error', src: 'app',     msg: '    at FlatList (cart.tsx:21:8)' },
  { t: '14:02:33.844', lvl: 'log',   src: 'app',     msg: '⏸︎ Paused on breakpoint cart.tsx:12' },
];

window.MOCK_COMPONENT_TREE = [
  { name: 'App', kind: 'fn', depth: 0, children: [
    { name: 'NavigationContainer', kind: 'fn', depth: 1, children: [
      { name: 'TabNavigator', kind: 'fn', depth: 2, children: [
        { name: 'CartScreen', kind: 'fn', depth: 3, selected: true, children: [
          { name: 'View', kind: 'host', depth: 4, children: [
            { name: 'Text', kind: 'host', depth: 5, children: [] },
            { name: 'FlatList', kind: 'fn', depth: 5, children: [
              { name: 'CartItem', kind: 'fn', depth: 6, props: 'item·onRemove', children: [
                { name: 'View', kind: 'host', depth: 7, children: [] },
              ]},
              { name: 'CartItem', kind: 'fn', depth: 6, props: 'item·onRemove', children: [] },
              { name: 'CartItem', kind: 'fn', depth: 6, props: 'item·onRemove', children: [] },
            ]},
            { name: 'Pressable', kind: 'host', depth: 5, children: [
              { name: 'Text', kind: 'host', depth: 6, children: [] },
            ]},
            { name: 'CheckoutSheet', kind: 'fn', depth: 5, props: 'open·onClose', children: [] },
          ]},
        ]},
      ]},
    ]},
  ]},
];

window.MOCK_SELECTED_COMPONENT = {
  name: 'CartScreen',
  source: 'app/(tabs)/cart.tsx:7',
  props: [],
  hooks: [
    { name: 'useCart',    value: '{items: Array(3), total: 14.5, …}' },
    { name: 'useState',   value: 'sheetOpen: false' },
    { name: 'useEffect',  value: 'deps: [items]' },
  ],
  rendered: '8ms ago · 14 renders',
};

// Profiler — frame timeline
window.MOCK_FRAMES = (() => {
  const arr = [];
  for (let i = 0; i < 64; i++) {
    // mostly green (~16ms), some yellow, occasional red spike
    let v = 8 + Math.sin(i * 0.4) * 3 + Math.random() * 4;
    if (i === 18) v = 38;
    if (i === 19) v = 28;
    if (i === 41) v = 22;
    if (i === 52) v = 45;
    arr.push(v);
  }
  return arr;
})();

window.MOCK_FLAMEGRAPH = [
  // {label, w (px), depth, hot}
  { label: 'commitRoot',          w: 720, d: 0, hot: false },
    { label: 'commitMutationEffects',  w: 380, d: 1, hot: false, x: 0 },
      { label: 'CartScreen.render',    w: 220, d: 2, hot: true,  x: 0 },
        { label: 'FlatList.render',    w: 160, d: 3, hot: true,  x: 0 },
          { label: 'CartItem×3',       w: 110, d: 4, hot: false, x: 0 },
      { label: 'CheckoutSheet',        w: 130, d: 2, hot: false, x: 220 },
    { label: 'commitLayoutEffects',    w: 220, d: 1, hot: false, x: 380 },
      { label: 'useEffect callbacks',  w: 150, d: 2, hot: false, x: 380 },
    { label: 'commitPassive',          w: 120, d: 1, hot: false, x: 600 },
];

window.MOCK_COMMANDS = [
  { id: 'open-file',    cat: 'File',    label: 'Open File…',                      shortcut: '⌘O' },
  { id: 'new-file',     cat: 'File',    label: 'New File',                        shortcut: '⌘N' },
  { id: 'save',         cat: 'File',    label: 'Save',                            shortcut: '⌘S' },
  { id: 'close-tab',    cat: 'File',    label: 'Close Tab',                       shortcut: '⌘W' },
  { id: 'find',         cat: 'Edit',    label: 'Find in File',                    shortcut: '⌘F' },
  { id: 'find-proj',    cat: 'Edit',    label: 'Find in Project',                 shortcut: '⇧⌘F' },
  { id: 'rename',       cat: 'Edit',    label: 'Rename Symbol',                   shortcut: 'F2' },
  { id: 'fmt',          cat: 'Edit',    label: 'Format Document',                 shortcut: '⌥⇧F' },
  { id: 'run-debug',    cat: 'Debug',   label: 'Start Debugging',                 shortcut: 'F5' },
  { id: 'stop-debug',   cat: 'Debug',   label: 'Stop Debugging',                  shortcut: '⇧F5' },
  { id: 'toggle-bp',    cat: 'Debug',   label: 'Toggle Breakpoint',               shortcut: 'F9' },
  { id: 'step-over',    cat: 'Debug',   label: 'Step Over',                       shortcut: 'F10' },
  { id: 'step-into',    cat: 'Debug',   label: 'Step Into',                       shortcut: 'F11' },
  { id: 'reload',       cat: 'Expo',    label: 'Reload App',                      shortcut: '⌘R' },
  { id: 'shake',        cat: 'Expo',    label: 'Shake Device',                    shortcut: '⌃⌘Z' },
  { id: 'fast-refresh', cat: 'Expo',    label: 'Toggle Fast Refresh',             shortcut: '' },
  { id: 'restart-metro',cat: 'Expo',    label: 'Restart Metro Bundler',           shortcut: '' },
  { id: 'open-sim',     cat: 'View',    label: 'Show Simulator',                  shortcut: '⌘1' },
  { id: 'open-debug',   cat: 'View',    label: 'Show Debugger',                   shortcut: '⌘2' },
  { id: 'open-tree',    cat: 'View',    label: 'Show Component Tree',             shortcut: '⌘3' },
  { id: 'open-prof',    cat: 'View',    label: 'Show Profiler',                   shortcut: '⌘4' },
  { id: 'open-cons',    cat: 'View',    label: 'Toggle Console',                  shortcut: '⌘J' },
];

// Devices for the simulator picker
window.MOCK_DEVICES = [
  { name: 'iPhone 15 Pro',     os: 'iOS 17.4', running: true  },
  { name: 'iPhone 15',         os: 'iOS 17.4', running: false },
  { name: 'iPhone SE (3rd)',   os: 'iOS 17.4', running: false },
  { name: 'iPad Pro 11"',      os: 'iPadOS 17.4', running: false },
];
