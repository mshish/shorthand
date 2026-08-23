import React from "react";

interface BadgeProps {
  children: React.ReactNode;
  variant?: "primary" | "success" | "secondary";
  className?: string;
}

const Badge: React.FC<BadgeProps> = ({
  children,
  variant = "primary",
  className = "",
}) => {
  const variantClasses = {
    // `text-background` is required, not cosmetic: this variant sets a solid
    // accent fill and no foreground, so it inherits body ink. The accent is a
    // dark blue in the light theme, which put this at 2.47:1.
    primary: "bg-logo-primary text-background",
    success: "bg-green-500/20 text-green-400",
    secondary: "bg-mid-gray/20 text-text/70",
  };

  return (
    <span
      className={`inline-flex items-center px-3 py-1 rounded-full text-xs font-medium ${variantClasses[variant]} ${className}`}
    >
      {children}
    </span>
  );
};

export default Badge;
