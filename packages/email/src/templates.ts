/** Plain transactional templates — no branding assets, no tracking. */

export function verificationEmail(verifyUrl: string): {
  subject: string;
  html: string;
  text: string;
} {
  return {
    subject: "Verify your RepoNyx account",
    html: `<p>Welcome to RepoNyx. Confirm your email address to finish creating your account:</p><p><a href="${verifyUrl}">Verify my email</a></p><p>If the link does not work, paste this URL into your browser:</p><p>${verifyUrl}</p><p>This link expires in 24 hours. If you did not create a RepoNyx account, ignore this email.</p>`,
    text: `Welcome to RepoNyx. Confirm your email address to finish creating your account:\n\n${verifyUrl}\n\nThis link expires in 24 hours. If you did not create a RepoNyx account, ignore this email.`,
  };
}

export function passwordResetEmail(resetUrl: string): {
  subject: string;
  html: string;
  text: string;
} {
  return {
    subject: "Reset your RepoNyx password",
    html: `<p>You asked to reset your RepoNyx password:</p><p><a href="${resetUrl}">Reset my password</a></p><p>If the link does not work, paste this URL into your browser:</p><p>${resetUrl}</p><p>This link expires in 1 hour. If you did not ask for a reset, ignore this email — your password is unchanged.</p>`,
    text: `You asked to reset your RepoNyx password:\n\n${resetUrl}\n\nThis link expires in 1 hour. If you did not ask for a reset, ignore this email — your password is unchanged.`,
  };
}
