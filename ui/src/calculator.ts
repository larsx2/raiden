const TOKEN = /\s*(\d+\.?\d*|\.\d+|[-+*/×÷x%^()])\s*/y;
const OPERATOR = /^[-+*/×÷x%^]$/;
const LONGEST = 200;
const DATE_OR_PHONE =
  /^(\d{4}([-/])\d{1,2}\2\d{1,2}|\d{1,2}([-/])\d{1,2}\3\d{4}|\d{1,2}\/\d{1,2}\/\d{2}|\+?(\d{1,3}-)?\d{3}-\d{3}-\d{4}|\+?\d{3}-\d{4}|\d{5}-\d{4})$/;
const MOST_DIGITS = 30;
const SHOWN = 15;
const ROUGH_SHOWN = 12;
const TINY = 10n ** BigInt(SHOWN);
const MOST_BITS = 1024;

const tokensOf = (text: string): string[] | undefined => {
  const tokens: string[] = [];
  TOKEN.lastIndex = 0;
  while (TOKEN.lastIndex < text.length) {
    const found = TOKEN.exec(text);
    if (!found) {
      return undefined;
    }
    tokens.push(found[1]!);
  }
  return tokens;
};

type Value = { n: bigint; d: bigint; rough: boolean };

class NoAnswer extends Error {}

const fail = (): never => {
  throw new NoAnswer();
};

const absolute = (value: bigint) => (value < 0n ? -value : value);

const bits = (value: bigint) => absolute(value).toString(2).length;

const gcd = (a: bigint, b: bigint): bigint => {
  let [x, y] = [absolute(a), absolute(b)];
  while (y !== 0n) {
    [x, y] = [y, x % y];
  }
  return x;
};

const value = (n: bigint, d: bigint, rough = false): Value => {
  if (d === 0n) {
    return fail();
  }
  const sign = d < 0n ? -1n : 1n;
  const common = gcd(n, d) || 1n;
  const made = { n: (sign * n) / common, d: (sign * d) / common, rough };
  return bits(made.n) > MOST_BITS || bits(made.d) > MOST_BITS ? fail() : made;
};

const ONE = value(1n, 1n);
const HUNDRED = value(100n, 1n);

const multiply = (a: Value, b: Value) => value(a.n * b.n, a.d * b.d, a.rough || b.rough);
const divide = (a: Value, b: Value) => value(a.n * b.d, a.d * b.n, a.rough || b.rough);
const add = (a: Value, b: Value) => value(a.n * b.d + b.n * a.d, a.d * b.d, a.rough || b.rough);
const negate = (of: Value) => ({ ...of, n: -of.n });

const decimal = (text: string, rough = false): Value => {
  const [whole = "", part = ""] = text.split(".");
  return value(BigInt(`${whole}${part}` || "0"), 10n ** BigInt(part.length), rough);
};

const roughly = (number: number): Value => {
  const [, sign, mantissa, exponent = "0"] = /^(-?)([\d.]+)(?:e([+-]\d+))?$/.exec(String(number))!;
  const shift = value(10n ** BigInt(Math.abs(Number(exponent))), 1n);
  const unsigned = decimal(mantissa!, true);
  const scaled = Number(exponent) < 0 ? divide(unsigned, shift) : multiply(unsigned, shift);
  return sign ? negate(scaled) : scaled;
};

const raise = (base: Value, power: Value): Value => {
  const times = absolute(power.n);
  if (power.d === 1n && BigInt(bits(base.n) + bits(base.d)) * times <= BigInt(MOST_BITS)) {
    const raised = value(base.n ** times, base.d ** times, base.rough || power.rough);
    return power.n < 0n ? divide(ONE, raised) : raised;
  }
  const x = Number(base.n) / Number(base.d);
  const y = Number(power.n) / Number(power.d);
  const result = x ** y;
  const lostBase = !Number.isFinite(x) || (x === 0 && base.n !== 0n);
  const lostResult = result === 0 && x !== 0;
  if (!Number.isFinite(result) || lostBase || lostResult || (x < 0 && !Number.isSafeInteger(y))) {
    return fail();
  }
  return roughly(result);
};

const evaluate = (tokens: string[]): Value => {
  let at = 0;
  let percentage = false;
  const peek = () => tokens[at];
  const take = () => tokens[at++];

  const sum = (): Value => {
    let total = product();
    while (peek() === "+" || peek() === "-") {
      const adds = take() === "+";
      percentage = false;
      const term = product();
      const change = percentage ? multiply(total, term) : term;
      total = add(total, adds ? change : negate(change));
    }
    return total;
  };

  const product = (): Value => {
    let total = unary();
    for (;;) {
      const op = peek();
      if (op === "*" || op === "×" || op === "x") {
        take();
        total = multiply(total, unary());
        percentage = false;
      } else if (op === "/" || op === "÷") {
        take();
        total = divide(total, unary());
        percentage = false;
      } else {
        return total;
      }
    }
  };

  const unary = (): Value => {
    if (peek() === "-" || peek() === "+") {
      return take() === "-" ? negate(unary()) : unary();
    }
    const base = percent();
    if (peek() !== "^") {
      return base;
    }
    take();
    const raised = raise(base, unary());
    percentage = false;
    return raised;
  };

  const percent = (): Value => {
    let total = atom();
    while (peek() === "%") {
      take();
      total = divide(total, HUNDRED);
      percentage = true;
    }
    return total;
  };

  const atom = (): Value => {
    const token = take();
    if (token === "(") {
      const inner = sum();
      percentage = false;
      return take() === ")" ? inner : fail();
    }
    return token !== undefined && /^[\d.]/.test(token) ? decimal(token) : fail();
  };

  const answer = sum();
  return at === tokens.length ? answer : fail();
};

const withPoint = (scaled: bigint, places: number) => {
  const text = scaled.toString().padStart(places + 1, "0");
  const whole = text.slice(0, text.length - places);
  const part = text.slice(text.length - places).replace(/0+$/, "");
  return part ? `${whole}.${part}` : whole;
};

const inFull = (size: bigint, d: bigint): string | undefined => {
  let rest = d;
  let twos = 0;
  let fives = 0;
  for (; rest % 2n === 0n; rest /= 2n) twos++;
  for (; rest % 5n === 0n; rest /= 5n) fives++;
  if (rest !== 1n) {
    return undefined;
  }
  const places = Math.max(twos, fives);
  const text = withPoint((size * 10n ** BigInt(places)) / d, places);
  return text.replace(".", "").replace(/^0+/, "").length <= MOST_DIGITS ? text : undefined;
};

const shown = ({ n, d, rough }: Value): string | undefined => {
  if (n === 0n) {
    return "0";
  }
  const size = absolute(n);
  if (size * TINY < d) {
    return undefined;
  }
  const sign = n < 0n ? "-" : "";
  const full = rough ? undefined : inFull(size, d);
  if (full !== undefined) {
    return sign + full;
  }
  const whole = size / d;
  let places = (rough ? ROUGH_SHOWN : SHOWN) - (whole === 0n ? 0 : whole.toString().length);
  if (whole === 0n) {
    for (let scaled = size * 10n; scaled < d; scaled *= 10n) places++;
  }
  if (places < 0) {
    return undefined;
  }
  const scaled = (size * 10n ** BigInt(places) * 2n + d) / (2n * d);
  const looksWhole = scaled % 10n ** BigInt(places) === 0n;
  return !rough && looksWhole ? undefined : sign + withPoint(scaled, places);
};

export function calculate(text: string): string | undefined {
  if (
    text.length > LONGEST ||
    /(^|[^\d.])0x|%\s*%/i.test(text) ||
    DATE_OR_PHONE.test(text.trim())
  ) {
    return undefined;
  }
  const tokens = tokensOf(text);
  const operates = tokens?.some(
    (token, at) => OPERATOR.test(token) && at > 0 && !/^[-+*/×÷x^(]$/.test(tokens[at - 1]!),
  );
  if (!tokens || !operates) {
    return undefined;
  }
  try {
    return shown(evaluate(tokens));
  } catch (error) {
    if (error instanceof NoAnswer) {
      return undefined;
    }
    throw error;
  }
}
