const TOKEN = /\s*(\d+\.?\d*|\.\d+|[-+*/×÷x%^()])\s*/y;
const OPERATOR = /^[-+*/×÷x%^]$/;
const LONGEST = 200;

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

const evaluate = (tokens: string[]): number => {
  let at = 0;
  const peek = () => tokens[at];
  const take = () => tokens[at++];

  const sum = (): number => {
    let value = product();
    while (peek() === "+" || peek() === "-") {
      value = take() === "+" ? value + product() : value - product();
    }
    return value;
  };

  const product = (): number => {
    let value = unary();
    for (;;) {
      const op = peek();
      if (op === "*" || op === "×" || op === "x") {
        take();
        value *= unary();
      } else if (op === "/" || op === "÷") {
        take();
        value /= unary();
      } else {
        return value;
      }
    }
  };

  const unary = (): number => {
    if (peek() === "-" || peek() === "+") {
      return take() === "-" ? -unary() : unary();
    }
    const base = percent();
    return peek() === "^" ? (take(), base ** unary()) : base;
  };

  const percent = (): number => {
    let value = atom();
    while (peek() === "%") {
      take();
      value /= 100;
    }
    return value;
  };

  const atom = (): number => {
    const token = take();
    if (token === "(") {
      const value = sum();
      return take() === ")" ? value : NaN;
    }
    return token !== undefined && /^[\d.]/.test(token) ? Number(token) : NaN;
  };

  const value = sum();
  return at === tokens.length ? value : NaN;
};

export function calculate(text: string): string | undefined {
  if (text.length > LONGEST || /0x/i.test(text)) {
    return undefined;
  }
  const tokens = tokensOf(text);
  const operates = tokens?.some(
    (token, at) => OPERATOR.test(token) && at > 0 && !/^[-+*/×÷x^(]$/.test(tokens[at - 1]!),
  );
  if (!tokens || !operates) {
    return undefined;
  }
  const value = evaluate(tokens);
  if (!Number.isFinite(value)) {
    return undefined;
  }
  return Number.isInteger(value) ? String(value) : String(Number(value.toPrecision(12)));
}
