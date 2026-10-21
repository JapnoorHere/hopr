//! Authentication for one hop: tries each credential in order, asking the user (through the
//! session state) only for what it doesn't know.

use hopr_core::session::PromptAnswer;
use hopr_core::{AppError, AuthPrompt, CloseReason, ErrorCode, PromptField, SessionState};
use russh::client::{AuthResult, Handle, KeyboardInteractiveAuthResponse};
use russh::keys::PrivateKeyWithHashAlg;
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::{mpsc, oneshot, watch};

use crate::connect::{Client, Question};
use crate::{Credential, Hop, agent, totp};

/// Ends authentication early.
enum Abort {
    Cancelled,
    Fatal(AppError),
}

impl From<russh::Error> for Abort {
    fn from(e: russh::Error) -> Self {
        Abort::Fatal(
            AppError::new(
                ErrorCode::Disconnected,
                "The server closed the connection during login",
            )
            .with_detail(e.to_string()),
        )
    }
}

struct Auth<'a> {
    handle: &'a mut Handle<Client>,
    hop: &'a Hop,
    state: &'a watch::Sender<SessionState>,
    questions: &'a mpsc::Sender<Question>,
    /// Password typed at a prompt with "remember" ticked; saved after success.
    remember: Option<SecretString>,
}

pub(crate) async fn authenticate(
    handle: &mut Handle<Client>,
    hop: &Hop,
    state: &watch::Sender<SessionState>,
    questions: &mpsc::Sender<Question>,
) -> Result<(), SessionState> {
    let mut auth = Auth {
        handle,
        hop,
        state,
        questions,
        remember: None,
    };
    let mut tried = Vec::new();
    for cred in &hop.credentials {
        tried.push(cred.name());
        state.send_replace(SessionState::Authenticating {
            method: cred.name().into(),
        });
        let ok = match auth.attempt(cred).await {
            Ok(ok) => ok,
            Err(Abort::Cancelled) => {
                return Err(SessionState::Closed {
                    reason: CloseReason::UserClosed,
                });
            }
            Err(Abort::Fatal(error)) => return Err(SessionState::Failed { error }),
        };
        if ok {
            if let (Some(pw), Some(hook)) = (auth.remember.take(), &hop.on_save_password) {
                hook(pw);
            }
            return Ok(());
        }
    }
    tried.dedup();
    Err(SessionState::Failed {
        error: AppError::new(
            ErrorCode::AuthFailed,
            format!("{} rejected the login for {}", hop.host, hop.username),
        )
        .with_detail(format!("Tried: {}", tried.join(", "))),
    })
}

fn looks_like_password(label: &str) -> bool {
    let l = label.to_lowercase();
    l.contains("password") || l.contains("passphrase")
}

fn looks_like_otp(label: &str) -> bool {
    let l = label.to_lowercase();
    [
        "verification",
        "one-time",
        "otp",
        "token",
        "code",
        "2fa",
        "authenticator",
        "mfa",
    ]
    .iter()
    .any(|w| l.contains(w))
}

impl Auth<'_> {
    /// `Ok(true)` = fully authenticated; `Ok(false)` = try the next method.
    async fn attempt(&mut self, cred: &Credential) -> Result<bool, Abort> {
        let user = self.hop.username.clone();
        match cred {
            Credential::Password(pw) => Ok(self
                .handle
                .authenticate_password(user, pw.expose_secret())
                .await?
                .success()),
            Credential::Key(key) => {
                let hash = if key.algorithm().is_rsa() {
                    self.handle.best_supported_rsa_hash().await?.flatten()
                } else {
                    None
                };
                Ok(self
                    .handle
                    .authenticate_publickey(user, PrivateKeyWithHashAlg::new(key.clone(), hash))
                    .await?
                    .success())
            }
            Credential::Certificate { key, cert } => Ok(self
                .handle
                .authenticate_openssh_cert(user, key.clone(), (**cert).clone())
                .await?
                .success()),
            Credential::Agent => Ok(agent::authenticate(self.handle, &user).await),
            Credential::PromptPassword => self.prompt_password().await,
            Credential::KeyboardInteractive {
                password,
                totp_seed,
            } => {
                self.keyboard_interactive(password.as_ref(), totp_seed.as_ref())
                    .await
            }
        }
    }

    async fn ask(&self, prompt: AuthPrompt) -> Result<PromptAnswer, Abort> {
        let (tx, rx) = oneshot::channel();
        self.questions
            .send(Question::Prompt { prompt, reply: tx })
            .await
            .map_err(|_| Abort::Cancelled)?;
        rx.await.ok().flatten().ok_or(Abort::Cancelled)
    }

    async fn prompt_password(&mut self) -> Result<bool, Abort> {
        let hop = self.hop;
        for attempt in 0..3 {
            let answer = self
                .ask(AuthPrompt {
                    title: format!("Password for {}@{}", hop.username, hop.host),
                    instructions: if attempt == 0 {
                        String::new()
                    } else {
                        "Incorrect password, try again.".into()
                    },
                    fields: vec![PromptField {
                        label: "Password".into(),
                        echo: false,
                    }],
                    can_remember: hop.on_save_password.is_some(),
                })
                .await?;
            let remember = answer.remember;
            let Some(pw) = answer.responses.into_iter().next() else {
                return Ok(false);
            };
            self.state.send_replace(SessionState::Authenticating {
                method: "password".into(),
            });
            match self
                .handle
                .authenticate_password(hop.username.clone(), pw.expose_secret())
                .await?
            {
                AuthResult::Success => {
                    if remember {
                        self.remember = Some(pw);
                    }
                    return Ok(true);
                }
                // Accepted but more is required (e.g. OTP next): let the next method run.
                AuthResult::Failure {
                    partial_success: true,
                    ..
                } => return Ok(false),
                AuthResult::Failure { .. } => {}
            }
        }
        Ok(false)
    }

    async fn keyboard_interactive(
        &mut self,
        password: Option<&SecretString>,
        totp_seed: Option<&SecretString>,
    ) -> Result<bool, Abort> {
        let user = self.hop.username.clone();
        let mut response = self
            .handle
            .authenticate_keyboard_interactive_start(user, None::<String>)
            .await?;
        let mut password_used = false;
        for _round in 0..6 {
            let (name, instructions, prompts) = match response {
                KeyboardInteractiveAuthResponse::Success => return Ok(true),
                KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
                KeyboardInteractiveAuthResponse::InfoRequest {
                    name,
                    instructions,
                    prompts,
                } => (name, instructions, prompts),
            };
            let mut answers: Vec<Option<SecretString>> = Vec::with_capacity(prompts.len());
            let mut ask_idx = Vec::new();
            for (i, p) in prompts.iter().enumerate() {
                let auto = if looks_like_password(&p.prompt) && !password_used {
                    password.map(|pw| {
                        password_used = true;
                        pw.clone()
                    })
                } else if looks_like_otp(&p.prompt) {
                    totp_seed
                        .and_then(|s| totp::current_code(s.expose_secret()))
                        .map(SecretString::from)
                } else {
                    None
                };
                if auto.is_none() {
                    ask_idx.push(i);
                }
                answers.push(auto);
            }
            if !ask_idx.is_empty() {
                let can_remember = self.hop.on_save_password.is_some()
                    && ask_idx.iter().any(|&i| {
                        prompts
                            .get(i)
                            .is_some_and(|p| looks_like_password(&p.prompt))
                    });
                let answer = self
                    .ask(AuthPrompt {
                        title: if name.is_empty() {
                            format!("{}@{}", self.hop.username, self.hop.host)
                        } else {
                            name.clone()
                        },
                        instructions: instructions.clone(),
                        fields: ask_idx
                            .iter()
                            .filter_map(|&i| prompts.get(i))
                            .map(|p| PromptField {
                                label: p.prompt.trim().to_owned(),
                                echo: p.echo,
                            })
                            .collect(),
                        can_remember,
                    })
                    .await?;
                for (slot, value) in ask_idx.iter().zip(answer.responses) {
                    if answer.remember
                        && prompts
                            .get(*slot)
                            .is_some_and(|p| looks_like_password(&p.prompt))
                    {
                        self.remember = Some(value.clone());
                    }
                    if let Some(a) = answers.get_mut(*slot) {
                        *a = Some(value);
                    }
                }
                self.state.send_replace(SessionState::Authenticating {
                    method: "keyboard-interactive".into(),
                });
            }
            let responses = answers
                .into_iter()
                .map(|a| a.map(|s| s.expose_secret().to_owned()).unwrap_or_default())
                .collect();
            response = self
                .handle
                .authenticate_keyboard_interactive_respond(responses)
                .await?;
        }
        Ok(false)
    }
}
