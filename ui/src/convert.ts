export type Rates = Record<string, number>;

export type Conversion = { from: string; to: string; value: string };

type Unit = { kind: string; symbol: string; factor: number; offset?: number };

const UNITS: Record<string, [string, number, string[]][]> = {
  length: [
    ["mm", 0.001, ["millimeter", "millimeters", "millimetre", "millimetres"]],
    ["cm", 0.01, ["centimeter", "centimeters", "centimetre", "centimetres"]],
    ["m", 1, ["meter", "meters", "metre", "metres"]],
    ["km", 1000, ["kilometer", "kilometers", "kilometre", "kilometres"]],
    ["in", 0.0254, ["inch", "inches", '"']],
    ["ft", 0.3048, ["foot", "feet", "'"]],
    ["yd", 0.9144, ["yard", "yards"]],
    ["mi", 1609.344, ["mile", "miles"]],
    ["nmi", 1852, ["nautical mile", "nautical miles"]],
  ],
  weight: [
    ["mg", 1e-6, ["milligram", "milligrams"]],
    ["g", 0.001, ["gram", "grams"]],
    ["kg", 1, ["kilogram", "kilograms", "kilo", "kilos"]],
    ["t", 1000, ["tonne", "tonnes", "metric ton", "metric tons"]],
    ["oz", 0.028349523125, ["ounce", "ounces"]],
    ["lb", 0.45359237, ["lbs", "pound", "pounds"]],
    ["st", 6.35029318, ["stone", "stones"]],
  ],
  volume: [
    ["ml", 0.001, ["mL", "milliliter", "milliliters", "millilitre", "millilitres"]],
    ["l", 1, ["L", "liter", "liters", "litre", "litres"]],
    ["tsp", 0.00492892159375, ["teaspoon", "teaspoons"]],
    ["tbsp", 0.01478676478125, ["tablespoon", "tablespoons"]],
    ["floz", 0.0295735295625, ["fl oz", "fluid ounce", "fluid ounces"]],
    ["cup", 0.2365882365, ["cups"]],
    ["pt", 0.473176473, ["pint", "pints"]],
    ["qt", 0.946352946, ["quart", "quarts"]],
    ["gal", 3.785411784, ["gallon", "gallons"]],
    ["m³", 1000, ["m3", "cubic meter", "cubic meters"]],
  ],
  area: [
    ["cm²", 1e-4, ["cm2", "sq cm", "square centimeter", "square centimeters"]],
    ["m²", 1, ["m2", "sq m", "sqm", "square meter", "square meters"]],
    ["km²", 1e6, ["km2", "sq km", "square kilometer", "square kilometers"]],
    ["ha", 1e4, ["hectare", "hectares"]],
    ["in²", 0.00064516, ["in2", "sq in", "square inch", "square inches"]],
    ["ft²", 0.09290304, ["ft2", "sq ft", "sqft", "square foot", "square feet"]],
    ["yd²", 0.83612736, ["yd2", "sq yd", "square yard", "square yards"]],
    ["ac", 4046.8564224, ["acre", "acres"]],
    ["mi²", 2589988.110336, ["mi2", "sq mi", "square mile", "square miles"]],
  ],
  speed: [
    ["m/s", 1, ["mps", "meters per second"]],
    ["km/h", 1 / 3.6, ["kmh", "kph", "kilometers per hour"]],
    ["mph", 0.44704, ["mi/h", "miles per hour"]],
    ["kn", 1852 / 3600, ["kt", "kts", "knot", "knots"]],
    ["ft/s", 0.3048, ["fps", "feet per second"]],
  ],
  pressure: [
    ["Pa", 1, ["pascal", "pascals"]],
    ["kPa", 1000, ["kilopascal", "kilopascals"]],
    ["MPa", 1e6, ["megapascal", "megapascals"]],
    ["bar", 1e5, ["bars"]],
    ["mbar", 100, ["millibar", "millibars", "hPa", "hectopascal", "hectopascals"]],
    ["atm", 101325, ["atmosphere", "atmospheres"]],
    ["psi", 6894.757293168, []],
    ["mmHg", 133.322387415, ["torr"]],
    ["inHg", 3386.389, []],
  ],
  time: [
    ["ms", 0.001, ["millisecond", "milliseconds"]],
    ["s", 1, ["sec", "secs", "second", "seconds"]],
    ["min", 60, ["mins", "minute", "minutes"]],
    ["h", 3600, ["hr", "hrs", "hour", "hours"]],
    ["d", 86400, ["day", "days"]],
    ["wk", 604800, ["week", "weeks"]],
  ],
  data: [
    ["B", 1, ["byte", "bytes"]],
    ["KB", 1e3, ["kilobyte", "kilobytes"]],
    ["MB", 1e6, ["megabyte", "megabytes"]],
    ["GB", 1e9, ["gigabyte", "gigabytes"]],
    ["TB", 1e12, ["terabyte", "terabytes"]],
    ["KiB", 1024, ["kibibyte", "kibibytes"]],
    ["MiB", 1024 ** 2, ["mebibyte", "mebibytes"]],
    ["GiB", 1024 ** 3, ["gibibyte", "gibibytes"]],
    ["TiB", 1024 ** 4, ["tebibyte", "tebibytes"]],
  ],
  energy: [
    ["J", 1, ["joule", "joules"]],
    ["kJ", 1000, ["kilojoule", "kilojoules"]],
    ["cal", 4.184, ["calorie", "calories"]],
    ["kcal", 4184, ["kilocalorie", "kilocalories"]],
    ["Wh", 3600, ["watt hour", "watt hours"]],
    ["kWh", 3.6e6, ["kilowatt hour", "kilowatt hours"]],
    ["BTU", 1055.05585262, ["btu", "btus"]],
  ],
  power: [
    ["W", 1, ["watt", "watts"]],
    ["kW", 1000, ["kilowatt", "kilowatts"]],
    ["hp", 745.69987158227, ["horsepower"]],
  ],
};

const TEMPERATURES: [string, number, number, string[]][] = [
  ["°C", 1, 273.15, ["c", "celsius", "degc"]],
  ["°F", 5 / 9, 459.67 * (5 / 9), ["f", "fahrenheit", "degf"]],
  ["K", 1, 0, ["kelvin", "kelvins"]],
];

const CURRENCY_NAMES: Record<string, string> = {
  $: "USD",
  dollar: "USD",
  dollars: "USD",
  "€": "EUR",
  euro: "EUR",
  euros: "EUR",
  "£": "GBP",
  "¥": "JPY",
  yen: "JPY",
  peso: "MXN",
  pesos: "MXN",
};

const EXACT = new Map<string, Unit>();
const FOLDED = new Map<string, Unit | null>();
const add = (name: string, unit: Unit) => {
  EXACT.set(name, unit);
  const folded = name.toLowerCase();
  const before = FOLDED.get(folded);
  FOLDED.set(folded, before === undefined || before?.symbol === unit.symbol ? unit : null);
};
for (const [kind, units] of Object.entries(UNITS)) {
  for (const [symbol, factor, names] of units) {
    for (const name of [symbol, ...names]) {
      add(name, { kind, symbol, factor });
    }
  }
}
for (const [symbol, factor, offset, names] of TEMPERATURES) {
  for (const name of [symbol, symbol.replace("°", ""), ...names]) {
    add(name, { kind: "temperature", symbol, factor, offset });
  }
}

const MEANINGFUL_CASE = "mMbB";

const foldedUnit = (name: string): Unit | null | undefined => {
  const unit = FOLDED.get(name.toLowerCase());
  const oneCase = name === name.toLowerCase() || name === name.toUpperCase();
  if (!unit || oneCase || name.length > 3 || name.length !== unit.symbol.length) {
    return unit;
  }
  return [...name].every(
    (letter, at) => letter === unit.symbol[at] || !MEANINGFUL_CASE.includes(letter),
  )
    ? unit
    : null;
};

const unitOf = (name: string, rates: Rates): Unit | undefined => {
  const known = EXACT.get(name) ?? foldedUnit(name);
  if (known !== undefined) {
    return known ?? undefined;
  }
  const code = CURRENCY_NAMES[name.toLowerCase()] ?? name.toUpperCase();
  const rate = rates[code];
  return rate ? { kind: "currency", symbol: code, factor: 1 / rate } : undefined;
};

const LONGEST = 200;
const AMOUNT = /^\s*([$€£¥]?)\s*(-?\d[\d.,]*|-?\.\d+)\s*(.*?)\s*$/;
const SEPARATOR = /\s+(?:to|in|as|=|→|->)(?=\s)/gi;
const GROUPED = /^-?[1-9]\d{0,2}(,\d{3})+(\.\d+)?$/;

const numberOf = (text: string) =>
  Number(GROUPED.test(text) ? text.replaceAll(",", "") : text.replace(",", "."));

const plain = (value: number) =>
  Number.isInteger(value) && Math.abs(value) <= Number.MAX_SAFE_INTEGER
    ? String(value)
    : value.toLocaleString("en-US", { maximumSignificantDigits: 12, useGrouping: false });

const money = (value: number, grouping: boolean) =>
  value !== 0 && Math.abs(value) < 0.01
    ? value.toLocaleString("en-US", { maximumSignificantDigits: 4, useGrouping: grouping })
    : value.toLocaleString("en-US", {
        maximumFractionDigits: 2,
        minimumFractionDigits: 2,
        useGrouping: grouping,
      });

const shown = (value: number, kind: string) =>
  kind === "currency" ? money(value, true) : plain(value);

const unitsOf = (sign: string, rest: string, rates: Rates): [Unit, Unit] | undefined => {
  for (const separator of rest.matchAll(SEPARATOR)) {
    const named = rest.slice(0, separator.index).trim();
    const from = named ? unitOf(named, rates) : sign ? unitOf(sign, rates) : undefined;
    const to = unitOf(rest.slice(separator.index + separator[0].length).trim(), rates);
    if (
      from &&
      to &&
      from.kind === to.kind &&
      (!sign || !named || unitOf(sign, rates)?.symbol === from.symbol)
    ) {
      return [from, to];
    }
  }
  return undefined;
};

export function convert(text: string, rates: Rates = {}): Conversion | undefined {
  const found = text.length <= LONGEST ? AMOUNT.exec(text) : null;
  if (!found) {
    return undefined;
  }
  const [, sign, digits, rest] = found;
  const units = unitsOf(sign!, ` ${rest!} `, rates);
  if (!units || units[0].symbol === units[1].symbol) {
    return undefined;
  }
  const [from, to] = units;
  const amount = numberOf(digits!);
  const base = amount * from.factor + (from.offset ?? 0);
  const value = (base - (to.offset ?? 0)) / to.factor;
  if (!Number.isFinite(value)) {
    return undefined;
  }
  return {
    from: `${shown(amount, from.kind)} ${from.symbol}`,
    to: `${shown(value, to.kind)} ${to.symbol}`,
    value: to.kind === "currency" ? money(value, false) : plain(value),
  };
}
