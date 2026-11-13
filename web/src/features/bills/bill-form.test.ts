import { describe, expect, it } from "vitest";
import {
  emptyBillForm,
  fromBill,
  shiftDate,
  toCreateRequest,
  toPatch,
  type BillForm,
} from "./bill-form";

const BILL = {
  id: "b1",
  name: "Rent",
  amount_min: "800.00",
  amount_max: "850.00",
  currency: "EUR",
  first_due_date: "2026-01-02",
  end_date: null,
  repeat_freq: "monthly",
  skip: 0,
  remind_days_before: 3,
  remind_every_days: 1,
  active: true,
  sort_order: 0,
  notes: "flat",
  next_due_date: "2026-12-02",
  created_at: "",
  updated_at: "",
};

describe("bill form", () => {
  it("starts empty with the household currency and default reminders", () => {
    const form = emptyBillForm("CHF", "2026-11-13");
    expect(form).toMatchObject({
      currency: "CHF",
      first_due_date: "2026-11-13",
      repeat_freq: "monthly",
      remind_days_before: "3",
      remind_every_days: "1",
    });
  });

  it("builds a create request, accepting a comma as decimal separator", () => {
    const form: BillForm = {
      ...emptyBillForm("EUR", "2026-11-13"),
      name: " Rent ",
      amount_min: "800",
      amount_max: "850,50",
      skip: "1",
      notes: "",
    };
    expect(toCreateRequest(form)).toEqual({
      name: "Rent",
      amount_min: "800",
      amount_max: "850.50",
      currency: "EUR",
      first_due_date: "2026-11-13",
      end_date: null,
      repeat_freq: "monthly",
      skip: 1,
      remind_days_before: 3,
      remind_every_days: 1,
      active: true,
      notes: null,
    });
  });

  it("round-trips a bill and patches only what changed", () => {
    const form = fromBill(BILL);
    expect(toPatch(BILL, form)).toEqual({});
    const changed = { ...form, amount_max: "900", notes: "", end_date: "2027-12-31" };
    expect(toPatch(BILL, changed)).toEqual({
      amount_max: "900",
      notes: null,
      end_date: "2027-12-31",
    });
  });

  it("shifts dates across month and year ends", () => {
    expect(shiftDate("2026-12-20", 15)).toBe("2027-01-04");
    expect(shiftDate("2026-03-01", -1)).toBe("2026-02-28");
  });
});
