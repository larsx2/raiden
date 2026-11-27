import { describe, expect, it } from "vitest";
import { calculate } from "./calculator";

describe("calculate", () => {
  it("answers simple arithmetic with the usual precedence", () => {
    expect(calculate("2+2")).toBe("4");
    expect(calculate("2 + 3 * 4")).toBe("14");
    expect(calculate("(2 + 3) * 4")).toBe("20");
    expect(calculate("10 / 4")).toBe("2.5");
    expect(calculate("-3 - -2")).toBe("-1");
    expect(calculate("2^10")).toBe("1024");
    expect(calculate("-2^2")).toBe("-4");
    expect(calculate("3 x 4")).toBe("12");
    expect(calculate("8 ÷ 2 × 3")).toBe("12");
    expect(calculate("50%")).toBe("0.5");
    expect(calculate("200 * 15%")).toBe("30");
    expect(calculate("0.1 + 0.2")).toBe("0.3");
    expect(calculate(".5*2")).toBe("1");
    expect(calculate("-2 * 3")).toBe("-6");
    expect(calculate("2^40+1")).toBe("1099511627777");
    expect(calculate("1000000000000.5+1")).toBe("1000000000001.5");
    expect(calculate("100 + 10%")).toBe("110");
    expect(calculate("80 - 25%")).toBe("60");
    expect(calculate("10% + 5")).toBe("5.1");
    expect(calculate("4 - 2 * 10%")).toBe("3.8");
    expect(calculate("12 - 3")).toBe("9");
    expect(calculate("12-3")).toBe("9");
    expect(calculate("1/4")).toBe("0.25");
    expect(calculate("11/27")).toBe("0.407407407407407");
    expect(calculate("5 + (100 + 10%)")).toBe("115");
    expect(calculate("100 + 2^10%")).toBe("101.071773462536");
    expect(calculate("0.0000001*2")).toBe("0.0000002");
    expect(calculate("0 * -1")).toBe("0");
  });

  it("stays quiet for anything that is not arithmetic", () => {
    for (const text of [
      "",
      "42",
      "(7)",
      "safari",
      "x",
      "2+",
      "(2+3",
      "2 3",
      "1/0",
      "xcode",
      "2+2 apples",
      "-7",
      "--2",
      "-(2)",
      "0x10",
      "2024-01-01",
      "555-1234",
      "2^70",
      "-".repeat(20000) + "1",
    ]) {
      expect(calculate(text)).toBeUndefined();
    }
  });
});
