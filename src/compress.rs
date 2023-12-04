use strum::EnumString;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::select;
use tracing::trace;

#[derive(Copy, Clone, Debug, Default, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum CompressAlgo {
    #[default]
    None,
    Lz4,
}

/// this could be generic, but we don't need it to be
pub async fn copy_bidirectional_with_compression(
    compress_algo: CompressAlgo,
    mut recv_q: quinn::RecvStream,
    mut send_q: quinn::SendStream,
    t: TcpStream,
) -> anyhow::Result<(u64, u64)> {
    // // TODO: use counters type
    // let mut a_to_b = AtomicU64::new(0);
    // let mut b_to_a = AtomicU64::new(0);
    // let mut compressed_a_to_b = AtomicU64::new(0);
    // let mut compressed_b_to_a = AtomicU64::new(0);

    let (mut recv_t, mut send_t) = t.into_split();

    // read from a, compress, write to b
    let a_to_b_f = async move {
        copy_with_compression(
            &mut recv_q,
            &mut send_t,
            CompressDirection::Decompress(compress_algo),
        )
        .await
    };

    // read from b, decompress, write to a
    let b_to_a_f = async move {
        copy_with_compression(
            &mut recv_t,
            &mut send_q,
            CompressDirection::Compress(compress_algo),
        )
        .await
    };

    let a_to_b_f = tokio::spawn(a_to_b_f);
    let b_to_a_f = tokio::spawn(b_to_a_f);

    select! {
        x = a_to_b_f => {
            trace!(?x, "a_to_b finished");
        },
        x = b_to_a_f => {
            trace!(?x, "b_to_a finished");
        },
    }

    // TODO: get this
    let (a_to_b, b_to_a) = (0, 0);

    Ok((a_to_b, b_to_a))
}

#[derive(Clone, Copy, Debug)]
pub enum CompressDirection {
    None,
    Compress(CompressAlgo),
    Decompress(CompressAlgo),
}

async fn copy_with_compression<R: AsyncRead + Unpin + ?Sized, W: AsyncWrite + Unpin + ?Sized>(
    r: &mut R,
    w: &mut W,
    d: CompressDirection,
) -> anyhow::Result<()> {
    // if compression is disabled, just use copy_bidirectional to avoid buffering

    let mut read_buf = [0; 8096];

    loop {
        let n = r.read(&mut read_buf).await?;

        trace!("read {} bytes. {:?}", n, d);

        // if n == 0 {
        //     trace!("closing");

        //     w.write_all(&[]).await?;

        //     // close if they send us 0
        //     break;
        // }

        let n_written = match d {
            CompressDirection::None
            | CompressDirection::Compress(CompressAlgo::None)
            | CompressDirection::Decompress(CompressAlgo::None) => {
                // a_to_b.fetch_add(n as u64, atomic::Ordering::SeqCst);

                w.write_all(&read_buf[..n]).await?;

                n
            }
            CompressDirection::Compress(CompressAlgo::Lz4) => {
                // a_to_b.fetch_add(n as u64, atomic::Ordering::SeqCst);

                let compressed = lz4_flex::compress_prepend_size(&read_buf[..n]);

                // compressed_a_to_b.fetch_add(compressed.len() as u64, atomic::Ordering::SeqCst);

                w.write_all(&compressed).await?;

                compressed.len()
            }
            CompressDirection::Decompress(CompressAlgo::Lz4) => {
                // compressed_a_to_b.fetch_add(n as u64, atomic::Ordering::SeqCst);

                let decompressed = lz4_flex::decompress_size_prepended(&read_buf[..n])
                    .map_err(|err| anyhow::anyhow!("decompress err: {:?}", err))?;

                // a_to_b.fetch_add(n as u64, atomic::Ordering::SeqCst);

                w.write_all(&decompressed).await?;

                decompressed.len()
            }
        };

        trace!("a -> b = {} -> {}", n, n_written);

        if n == 0 {
            trace!("closing");
            break;
        }
    }

    Ok(())
}
