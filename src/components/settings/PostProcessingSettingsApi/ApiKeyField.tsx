import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "../../ui/Input";
import { Button } from "../../ui/Button";
import type { CredentialStatus } from "@/bindings";

interface ApiKeyFieldProps {
  status: CredentialStatus;
  onCommit: (value: string) => Promise<void>;
  onClear: () => void;
  disabled: boolean;
  className?: string;
}

// Write-only: the settings payload the backend sends down never carries a
// saved key's value, so this field never has one to show. It only ever
// sends what the user just typed.
export const ApiKeyField: React.FC<ApiKeyFieldProps> = React.memo(
  ({ status, onCommit, onClear, disabled, className = "" }) => {
    const { t } = useTranslation();
    const [value, setValue] = useState("");

    const placeholder =
      status === "configured"
        ? t("shorthand.apiKey.saved")
        : status === "unavailable"
          ? t("shorthand.apiKey.unavailable")
          : t("shorthand.apiKey.missing");

    const handleBlur = async () => {
      if (value === "") return;
      try {
        await onCommit(value);
        setValue("");
      } catch {
        // onCommit already reports the error; leave the value in place so
        // the user does not have to find and re-paste the key.
      }
    };

    return (
      <>
        <Input
          type="password"
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onBlur={handleBlur}
          placeholder={placeholder}
          variant="compact"
          disabled={disabled}
          className={`flex-1 min-w-[320px] ${className}`}
        />
        {status === "configured" && (
          <Button
            type="button"
            variant="secondary"
            size="sm"
            disabled={disabled}
            onClick={onClear}
          >
            {t("shorthand.apiKey.clear")}
          </Button>
        )}
      </>
    );
  },
);

ApiKeyField.displayName = "ApiKeyField";
