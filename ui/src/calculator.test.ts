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
    expect(calculate("1920x1080")).toBe("2073600");
    expect(calculate("500-100")).toBe("400");
    expect(calculate("9007199254740990+1")).toBe("9007199254740991");
    expect(calculate("1234567890123456+1")).toBe("1234567890123457");
    expect(calculate("99999999999999.5+0.25")).toBe("99999999999999.75");
    expect(calculate("1000000.1-1000000")).toBe("0.1");
    expect(calculate("1.00000000000001-1")).toBe("0.00000000000001");
    expect(calculate("12345678901234567-12345678901234566")).toBe("1");
    expect(calculate("2^60+1-2^60")).toBe("1");
    expect(calculate("2^53-1")).toBe("9007199254740991");
    expect(calculate("2^70")).toBe("1180591620717411303424");
    expect(calculate("9007199254740991/2")).toBe("4503599627370495.5");
    expect(calculate("4503599627370497-0.5")).toBe("4503599627370496.5");
    expect(calculate("123456789012345+0.5")).toBe("123456789012345.5");
    expect(calculate("9007199254740991%")).toBe("90071992547409.91");
    expect(calculate("1.1^40")).toBe("45.259255568176");
    expect(calculate("1^1000000000")).toBe("1");
    expect(calculate("(-1)^1000000001")).toBe("-1");
    expect(calculate("1-555-1234")).toBe("-1788");
    expect(calculate("100-1000000")).toBe("-999900");
    expect(calculate("(1+0.05/365)^365")).toBe("1.05126749647");
    expect(calculate("1.05^500")).toBe("39323261827");
    expect(calculate("2^0.5*1000000000-1414213562")).toBe("0.3731");
    expect(calculate("2^0.5-1")).toBe("0.4142135623731");
    expect(calculate("(2^0.5)^9")).toBe("22.62741699797");
    expect(calculate("1.000001^1000000")).toBe("2.71828");
    expect(calculate("0^5000")).toBe("0");
    expect(calculate("51^0.5")).toBe("7.141428428543");
    expect(calculate("65^0.5")).toBe("8.0622577483");
    expect(calculate("1.50000000000000000000*2")).toBe("3");
    expect(calculate("0.000000000000001*3")).toBe("0.000000000000003");
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
      "555-555-1234",
      "2 + 0x10",
      "2^-1074",
      "100+10%%",
      "2^200",
      "1.5^80",
      "9^9^9",
      "(-8)^0.5",
      "0^-1",
      "(-1)^100000000000000001",
      "0.5^1100",
      "(-2)^(1+10^-20)",
      "(-3)^(2+10^-20)",
      "12/31/24",
      "90210-1234",
      "(10^(1.0000000000000001^0.5)-10)*10^20",
      "99^120*99^120/99^120/99^120",
      "2^0.5-1.41421356237309",
      "0.9^512*".repeat(24) + "1",
      "50% %",
      "12-31-2024",
      "1/2/2024",
      "2024/01/01",
      "1-800-555-1234",
      "+1-800-555-1234",
      "0.30000000000000001-0.3",
      "+555-1234",
      "-".repeat(20000) + "1",
    ]) {
      expect(calculate(text)).toBeUndefined();
    }
  });
});
