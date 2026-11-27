const TOKEN = /\s*(\d+\.?\d*|\.\d+|[-+*/×÷x%^()])\s*/y;
const OPERATOR = /^[-+*/×÷x%^]$/;
const LONGEST = 200;
const SMALLEST = 1e-15;
const DATE_OR_PHONE = /^(\d{4}-\d{1,2}-\d{1,2}|\d{3}-\d{3,4}(-\d{4})?)$/;

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
  let percentage = false;
  const peek = () => tokens[at];
  const take = () => tokens[at++];

  const sum = (): number => {
    let value = product();
    while (peek() === "+" || peek() === "-") {
      const adds = take() === "+";
      percentage = false;
      const term = product();
      const change = percentage ? value * term : term;
      value = adds ? value + change : value - change;
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
        percentage = false;
      } else if (op === "/" || op === "÷") {
        take();
        value /= unary();
        percentage = false;
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
    if (peek() !== "^") {
      return base;
    }
    take();
    const raised = base ** unary();
    percentage = false;
    return raised;
  };

  const percent = (): number => {
    let value = atom();
    while (peek() === "%") {
      take();
      value /= 100;
      percentage = true;
    }
    return value;
  };

  const atom = (): number => {
    const token = take();
    if (token === "(") {
      const value = sum();
      percentage = false;
      return take() === ")" ? value : NaN;
    }
    return token !== undefined && /^[\d.]/.test(token) ? Number(token) : NaN;
  };

  const value = sum();
  return at === tokens.length ? value : NaN;
};

export function calculate(text: string): string | undefined {
  if (text.length > LONGEST || /0x|%%/i.test(text) || DATE_OR_PHONE.test(text.trim())) {
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
  const size = Math.abs(value);
  if (!Number.isFinite(value) || size > Number.MAX_SAFE_INTEGER || (size > 0 && size < SMALLEST)) {
    return undefined;
  }
  if (Number.isInteger(value)) {
    return String(value + 0);
  }
  return value.toLocaleString("en-US", { maximumSignificantDigits: 15, useGrouping: false });
}
