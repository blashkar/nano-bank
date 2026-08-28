"use client";

import React, { useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { toast } from "sonner";
import { CreditCard, Wallet, AlertCircle } from "lucide-react";
import { makeCreditCardPaymentAction } from "@/actions/credit";
import { Account } from "@/lib/accounts";
import SubmitButton from "@/components/SubmitButton";

const formatCurrency = (val: number) => {
  return new Intl.NumberFormat("en-CA", {
    style: "currency",
    currency: "CAD",
  }).format(val);
};

const accountLabel = (account: Account) => {
  const last4 = account.account_number.slice(-4);
  const type = account.account_type === "chequing" ? "Chequing" : "Savings";
  return `${type} ••${last4} — ${formatCurrency(parseFloat(account.balance))}`;
};

const cardLabel = (card: Account) => {
  const last4 = card.account_number.slice(-4);
  return `Visa ••${last4} — Balance: ${formatCurrency(parseFloat(card.balance))}`;
};

export default function PaymentForm({
  fundingAccounts,
  creditCards,
}: {
  fundingAccounts: Account[];
  creditCards: Account[];
}) {
  const router = useRouter();
  const [loading, setLoading] = useState(false);
  const [fromAccountId, setFromAccountId] = useState(fundingAccounts[0].account_id);
  const [toAccountId, setToAccountId] = useState(creditCards[0].account_id);
  const [amountType, setAmountType] = useState<"statement" | "current" | "custom">("current");
  const [customAmount, setCustomAmount] = useState("");
  const [notes, setNotes] = useState("");

  const selectedCard = useMemo(
    () => creditCards.find((c) => c.account_id === toAccountId) ?? creditCards[0],
    [creditCards, toAccountId]
  );

  const selectedFromAccount = useMemo(
    () => fundingAccounts.find((a) => a.account_id === fromAccountId) ?? fundingAccounts[0],
    [fundingAccounts, fromAccountId]
  );

  const availableFunds = parseFloat(selectedFromAccount.available_balance);

  const currentOwed = parseFloat(selectedCard.balance);
  const statementOwed = Math.round(currentOwed * 0.65 * 100) / 100;

  const chosenAmount = useMemo(() => {
    if (amountType === "statement") return statementOwed;
    if (amountType === "current") return currentOwed;
    return Number(customAmount) || 0;
  }, [amountType, statementOwed, currentOwed, customAmount]);

  const hasInsufficientFunds = chosenAmount > availableFunds;

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();

    let payValue = 0;
    if (amountType === "statement") {
      payValue = statementOwed;
    } else if (amountType === "current") {
      payValue = currentOwed;
    } else {
      payValue = Number(customAmount);
    }

    if (!Number.isFinite(payValue) || payValue <= 0) {
      toast.error("Please enter or select a valid payment amount greater than $0.00.");
      return;
    }

    if (availableFunds < payValue) {
      toast.error(`Insufficient funds. Your selected account only has ${formatCurrency(availableFunds)} available.`);
      return;
    }

    setLoading(true);
    const formData = new FormData(event.currentTarget);
    formData.append("statementAmountValue", statementOwed.toString());
    formData.append("currentAmountValue", currentOwed.toString());

    try {
      const response = await makeCreditCardPaymentAction(formData);
      if (response.success) {
        toast.success(response.message);
        router.push("/dashboard/credit");
        return;
      }
      toast.error(response.message);
    } catch (error) {
      console.error(error);
      toast.error("An unexpected error occurred while making card payment.");
    } finally {
      setLoading(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-6 w-full">
      {/* From Funding Account Dropdown */}
      <div className="space-y-2">
        <label htmlFor="fromAccountId" className="text-xs font-semibold tracking-wide text-slate-300 flex items-center gap-1.5">
          <Wallet className="w-4 h-4 text-nanobank-blue-sky" />
          <span>Pay From</span>
        </label>
        <select
          id="fromAccountId"
          name="fromAccountId"
          value={fromAccountId}
          onChange={(e) => setFromAccountId(e.target.value)}
          className="w-full p-3 rounded-lg border border-slate-700 bg-slate-900/50 text-sm text-white focus:outline-none focus:ring-2 focus:ring-nanobank-blue-sky/60 cursor-pointer"
        >
          {fundingAccounts.map((account) => (
            <option key={account.account_id} value={account.account_id}>
              {accountLabel(account)}
            </option>
          ))}
        </select>
      </div>

      {/* To Credit Card Dropdown */}
      <div className="space-y-2">
        <label htmlFor="toAccountId" className="text-xs font-semibold tracking-wide text-slate-300 flex items-center gap-1.5">
          <CreditCard className="w-4 h-4 text-nanobank-orange-deep" />
          <span>Pay To (Credit Card)</span>
        </label>
        <select
          id="toAccountId"
          name="toAccountId"
          value={toAccountId}
          onChange={(e) => setToAccountId(e.target.value)}
          className="w-full p-3 rounded-lg border border-slate-700 bg-slate-900/50 text-sm text-white focus:outline-none focus:ring-2 focus:ring-nanobank-blue-sky/60 cursor-pointer"
        >
          {creditCards.map((card) => (
            <option key={card.account_id} value={card.account_id}>
              {cardLabel(card)}
            </option>
          ))}
        </select>
      </div>

      {/* Payment Amount Selection Options */}
      <div className="space-y-3">
        <span className="text-xs font-semibold tracking-wide text-slate-300">
          Payment Amount Option
        </span>
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
          {/* Statement Balance Option */}
          <label
            className={`flex flex-col justify-between p-4 rounded-xl border cursor-pointer transition-all ${
              amountType === "statement"
                ? "border-nanobank-orange-deep bg-nanobank-orange-deep/10"
                : "border-slate-700 bg-slate-900/50 hover:border-slate-500"
            }`}
          >
            <input
              type="radio"
              name="amountType"
              value="statement"
              checked={amountType === "statement"}
              onChange={() => setAmountType("statement")}
              className="sr-only"
            />
            <div>
              <p className="text-xs font-semibold text-slate-400">Statement Balance</p>
              <p className="text-lg font-black text-white mt-1">
                {formatCurrency(statementOwed)}
              </p>
            </div>
            <p className="text-[10px] text-slate-500 mt-2">Estimated minimum/due amount</p>
          </label>

          {/* Current Balance Option */}
          <label
            className={`flex flex-col justify-between p-4 rounded-xl border cursor-pointer transition-all ${
              amountType === "current"
                ? "border-nanobank-orange-deep bg-nanobank-orange-deep/10"
                : "border-slate-700 bg-slate-900/50 hover:border-slate-500"
            }`}
          >
            <input
              type="radio"
              name="amountType"
              value="current"
              checked={amountType === "current"}
              onChange={() => setAmountType("current")}
              className="sr-only"
            />
            <div>
              <p className="text-xs font-semibold text-slate-400">Current Balance</p>
              <p className="text-lg font-black text-white mt-1">
                {formatCurrency(currentOwed)}
              </p>
            </div>
            <p className="text-[10px] text-slate-500 mt-2">Total outstanding owed amount</p>
          </label>

          {/* Custom Amount Option */}
          <label
            className={`flex flex-col justify-between p-4 rounded-xl border cursor-pointer transition-all ${
              amountType === "custom"
                ? "border-nanobank-orange-deep bg-nanobank-orange-deep/10"
                : "border-slate-700 bg-slate-900/50 hover:border-slate-500"
            }`}
          >
            <input
              type="radio"
              name="amountType"
              value="custom"
              checked={amountType === "custom"}
              onChange={() => setAmountType("custom")}
              className="sr-only"
            />
            <div>
              <p className="text-xs font-semibold text-slate-400">Custom Amount</p>
              <p className="text-lg font-black text-white mt-1">
                {customAmount ? formatCurrency(Number(customAmount) || 0) : "$0.00"}
              </p>
            </div>
            <p className="text-[10px] text-slate-500 mt-2">Enter any preferred amount</p>
          </label>
        </div>
      </div>

      {/* Dynamic Custom Amount Input */}
      {amountType === "custom" && (
        <div className="space-y-1.5 animate-in fade-in slide-in-from-top-1 duration-150">
          <label htmlFor="customAmount" className="text-xs font-semibold tracking-wide text-slate-300">
            Enter Custom Amount
          </label>
          <div className="relative">
            <span className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-500 text-sm">$</span>
            <input
              id="customAmount"
              name="customAmount"
              type="number"
              inputMode="decimal"
              step="0.01"
              required
              value={customAmount}
              onChange={(e) => setCustomAmount(e.target.value)}
              placeholder="0.00"
              className="w-full pl-7 pr-4 py-3 rounded-lg border border-slate-700 bg-slate-900/50 hover:border-slate-500 focus:border-nanobank-blue-sky focus:outline-none transition-colors duration-200 text-sm placeholder:text-slate-500"
            />
          </div>
        </div>
      )}

      {/* Optional Notes Field */}
      <div className="space-y-1.5">
        <label htmlFor="notes" className="text-xs font-semibold tracking-wide text-slate-300">
          Notes (Optional)
        </label>
        <input
          id="notes"
          name="notes"
          type="text"
          value={notes}
          onChange={(e) => setNotes(e.target.value)}
          placeholder="e.g. July Visa payment"
          className="w-full px-4 py-3 rounded-lg border border-slate-700 bg-slate-900/50 hover:border-slate-500 focus:border-nanobank-blue-sky focus:outline-none transition-colors duration-200 text-sm placeholder:text-slate-500"
        />
      </div>

      {/* Insufficient Funds Live Error */}
      {hasInsufficientFunds && chosenAmount > 0 && (
        <div className="flex items-start gap-3 p-4 rounded-xl border border-rose-500/20 bg-rose-500/10 text-rose-300 text-sm animate-in fade-in slide-in-from-top-1 duration-200">
          <AlertCircle className="w-5 h-5 flex-shrink-0 text-rose-400 mt-0.5" />
          <div>
            <span className="font-semibold text-rose-200">Insufficient Funds:</span> Your selected Pay From account only has <span className="font-extrabold text-white">{formatCurrency(availableFunds)}</span> available, which is less than the requested <span className="font-extrabold text-white">{formatCurrency(chosenAmount)}</span> payment.
          </div>
        </div>
      )}

      {/* Form Submission Button */}
      <SubmitButton loading={loading} loadingText="Processing Payment...">
        Pay Card
      </SubmitButton>
    </form>
  );
}
