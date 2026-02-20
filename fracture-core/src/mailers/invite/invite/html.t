<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family: system-ui, sans-serif; max-width: 560px; margin: 0 auto; padding: 2rem;">
  <h2>You're invited!</h2>
  <p><strong>{{ inviter_name }}</strong> has invited you to join <strong>{{ org_name }}</strong> as a <strong>{{ role }}</strong>.</p>
  <p><a href="{{ accept_url }}" style="display: inline-block; padding: 0.75rem 1.5rem; background: #62cb98; color: #fff; text-decoration: none; border-radius: 6px; font-weight: 600;">Accept Invite</a></p>
  <p style="font-size: 0.875rem; color: #6b7280;">Or copy this link: {{ accept_url }}</p>
  <p style="font-size: 0.875rem; color: #6b7280;">This invite expires in 7 days.</p>
  <hr style="border: none; border-top: 1px solid #e5e7eb; margin: 2rem 0;">
  <p style="font-size: 0.75rem; color: #9ca3af;">Fracture CMS</p>
</body>
</html>